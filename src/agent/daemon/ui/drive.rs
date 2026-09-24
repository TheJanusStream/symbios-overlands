//! Working the interface (#1424): one command at a time, frame by frame.
//!
//! Every action is an AccessKit request aimed at one widget by its id - a
//! click, a focus, a value, a scroll - so nothing but that widget can take
//! it: no pointer moves, nothing behind the widget can be hit, and no game
//! shortcut can fire, since shortcuts read Bevy's keys and only egui's
//! events are written. egui_ltreeview's rows take no request, so a row is
//! selected by a pointer press and release at its middle, once nothing else
//! is drawn there, and the pointer is taken away after.
//!
//! What a person could not do, the agent does not either. A control out of
//! view is scrolled into view first (egui centres it in its own scroll
//! area), and refused if that fails; one under another window has that
//! window raised first, as a person's click raises it; one behind a dialog,
//! or greyed out, is refused. AccessKit on its own would click all three.
//!
//! Each command leaves the interface as a person would expect to find it,
//! with no menu open and no field holding the keyboard: either holds the
//! game's keyboard, and the agent walks by pressing keys.

use std::collections::HashMap;

use bevy_egui::egui;
use bevy_egui::egui::accesskit::{
    Action, ActionData, ActionRequest, NodeId, Role, TreeId, TreeUpdate,
};
use serde_json::{Value, json};

use super::tree::{Entry, Kind, Surface, UiTree};

/// Frames a command may take before it is given up: three seconds at the
/// daemon's 30 Hz.
const BUDGET_FRAMES: u32 = 90;

/// Frames to wait after an action before reading what it did: the pass
/// that takes the action, and one more for what it raised to be drawn.
const SETTLE_FRAMES: u32 = 2;

/// ScrollUp/ScrollDown requests move a scroll area this far each (egui's
/// own step).
const SCROLL_STEP_POINTS: f32 = 100.0;

/// The most frames a scroll is given to come to rest. egui animates one
/// for up to 0.3 s - nine frames at the daemon's 30 Hz.
const SCROLL_SETTLE_FRAMES: u32 = 15;

/// What a command asks of the interface.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Verb {
    /// What is open, and what can be.
    Summary,
    /// One surface's listing.
    Show(String),
    /// Wait for the window titled so, which the caller has opened, to lay
    /// itself out, and list it.
    Open(String),
    /// Wait for the window titled so to go, which the caller has closed.
    Closed(String),
    /// Close the audio pop-out titled so by its own close button.
    CloseByButton(String),
    Click(String),
    Type {
        path: String,
        text: String,
        enter: bool,
    },
    Set {
        path: String,
        value: f64,
    },
    Choose {
        path: String,
        option: String,
    },
    Scroll {
        window: String,
        points: f32,
    },
}

impl Verb {
    /// Whether it may change one of the agent's records.
    pub(super) fn acts(&self) -> bool {
        matches!(
            self,
            Self::Click(_) | Self::Type { .. } | Self::Set { .. } | Self::Choose { .. }
        )
    }
}

/// Where a command stands after a frame.
#[derive(Debug)]
pub(super) enum Poll {
    Pending,
    Done(Result<Value, String>),
}

/// What one frame gives a command to work with.
pub(super) struct Frame<'a> {
    pub ctx: &'a egui::Context,
    /// The last pass's AccessKit tree, when it was built with AccessKit on.
    pub tree: Option<&'a TreeUpdate>,
    /// Events for the next pass to take.
    pub events: &'a mut Vec<egui::Event>,
    /// The dialogs and menus the agent's own clicks raised, by node.
    pub raised: &'a mut HashMap<NodeId, Surface>,
}

/// One command, worked frame by frame.
#[derive(Debug)]
pub(super) struct Driver {
    verb: Verb,
    frame: u32,
    /// Frames to let pass before the next look.
    wait: u32,
    scrolls: u32,
    raised_front: bool,
    delivered: bool,
    focus_tries: u32,
    typed: bool,
    released: bool,
    /// The frame a menu's opener was clicked, for `choose`.
    opened_at: Option<u32>,
    picked: Option<String>,
    /// The frame a window being opened was first seen.
    seen_at: Option<u32>,
    /// A row press to release on the next frame, and where.
    release_at: Option<egui::Pos2>,
    roots_before: Vec<NodeId>,
    toasts_before: Vec<String>,
    /// A field's value once typed into, before its focus was given back.
    typed_value: Option<String>,
    /// The whole path of the control a click was sent to, and the window
    /// it is in - which a dialog it raises belongs to, even when the click
    /// closed that window.
    target: Option<(String, String)>,
    /// Where a scrolled control stood last frame, and the frame the
    /// scroll was sent: egui animates a scroll, and it is done once the
    /// area stops moving.
    probe: Option<egui::Rect>,
    moved: bool,
    delivered_at: u32,
    /// The shown surface's rect, for a picture cropped to it.
    shown_rect: Option<egui::Rect>,
    /// Why a picture of the interface may not be taken now, if it may not.
    picture_blocker: Option<String>,
}

enum Aim {
    Ready,
    Wait,
    Refused(String),
}

impl Driver {
    pub(super) fn new(verb: Verb) -> Self {
        Self {
            verb,
            frame: 0,
            wait: 0,
            scrolls: 0,
            raised_front: false,
            delivered: false,
            focus_tries: 0,
            typed: false,
            released: false,
            opened_at: None,
            picked: None,
            seen_at: None,
            release_at: None,
            roots_before: Vec::new(),
            toasts_before: Vec::new(),
            typed_value: None,
            target: None,
            probe: None,
            moved: false,
            delivered_at: 0,
            shown_rect: None,
            picture_blocker: None,
        }
    }

    pub(super) fn verb(&self) -> &Verb {
        &self.verb
    }

    /// Where the surface `show` listed is drawn, in egui points.
    pub(super) fn shown_rect(&self) -> Option<egui::Rect> {
        self.shown_rect
    }

    /// Why a picture of the interface may not be taken now, as the listing
    /// found it.
    pub(super) fn picture_blocker(&self) -> Option<&str> {
        self.picture_blocker.as_deref()
    }

    /// One frame of the command.
    pub(super) fn step(&mut self, frame: Frame) -> Poll {
        self.frame += 1;
        if let Some(at) = self.release_at.take() {
            frame.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
            frame.events.push(egui::Event::PointerGone);
            self.wait = SETTLE_FRAMES;
            return Poll::Pending;
        }
        if self.frame > BUDGET_FRAMES {
            return Poll::Done(Err(format!(
                "the interface did not finish that within {BUDGET_FRAMES} frames"
            )));
        }
        if self.wait > 0 {
            self.wait -= 1;
            return Poll::Pending;
        }
        let Some(update) = frame.tree else {
            return Poll::Pending;
        };
        let screen = frame.ctx.content_rect();
        let Some(tree) = UiTree::read(update, screen, frame.raised) else {
            return Poll::Pending;
        };
        // A dialog or a menu that is gone is nobody's any more.
        frame.raised.retain(|node, _| tree.roots.contains(node));
        if matches!(self.verb, Verb::Summary | Verb::Show(_)) {
            self.picture_blocker = picture_blocker(&tree, update, frame.ctx, frame.raised);
        }
        match self.verb.clone() {
            Verb::Summary => Poll::Done(Ok(summary(&tree, frame.ctx))),
            Verb::Show(name) => {
                self.shown_rect = tree
                    .surface_named(&name)
                    .and_then(|index| tree.surfaces[index].rect);
                Poll::Done(show(&tree, &name))
            }
            Verb::Open(title) => self.open(&tree, frame.ctx, &title),
            // Gone from both lists: a window the agent may not read is
            // never one of its surfaces, and a closing window fades out
            // over a few frames.
            Verb::Closed(title) => {
                let still_drawn =
                    tree.surface_named(&title).is_some() || tree.refused_windows.contains(&title);
                if still_drawn {
                    Poll::Pending
                } else {
                    Poll::Done(Ok(json!({ "closed": title })))
                }
            }
            Verb::CloseByButton(title) => self.close_by_button(&tree, frame, &title),
            Verb::Click(path) => self.click(&tree, update, frame, &path),
            Verb::Type { path, text, enter } => self.type_into(&tree, frame, &path, &text, enter),
            Verb::Set { path, value } => self.set(&tree, frame, &path, value),
            Verb::Choose { path, option } => self.choose(&tree, update, frame, &path, &option),
            Verb::Scroll { window, points } => self.scroll(&tree, frame, &window, points),
        }
    }

    fn open(&mut self, tree: &UiTree, ctx: &egui::Context, title: &str) -> Poll {
        let Some(index) = tree.surface_named(title) else {
            return Poll::Pending;
        };
        let seen = *self.seen_at.get_or_insert(self.frame);
        if seen == self.frame {
            raise(ctx, tree.surfaces[index].node);
        }
        // egui lays a new window out on an invisible first pass, every
        // control in it disabled; its first real one comes after.
        if self.frame < seen + 2 {
            return Poll::Pending;
        }
        Poll::Done(Ok(tree.surface_json(index)))
    }

    fn close_by_button(&mut self, tree: &UiTree, frame: Frame, title: &str) -> Poll {
        let Some(index) = tree.surface_named(title) else {
            return Poll::Done(Ok(json!({ "closed": title })));
        };
        if self.delivered {
            return Poll::Pending;
        }
        let Some(close) = tree
            .entries_of(index)
            .find(|e| e.label.as_deref() == Some("Close window"))
        else {
            return Poll::Done(Err(format!("{title} has no close button")));
        };
        frame.events.push(request(close.node, Action::Click, None));
        self.delivered = true;
        Poll::Pending
    }

    /// Everything that keeps a person's hand off a control, checked before
    /// an action is sent: a dialog over it, its being out of view, another
    /// window over it.
    fn aim(&mut self, tree: &UiTree, frame: &mut Frame, entry: &Entry, pointer: bool) -> Aim {
        let target = &tree.surfaces[entry.surface];
        if let Some(modal) = frame.ctx.memory(|m| m.top_modal_layer())
            && !target.owns(modal)
        {
            return Aim::Refused(match tree.surfaces.iter().find(|s| s.owns(modal)) {
                Some(dialog) => format!(
                    "the {} is open, and nothing else can be used until it is answered; \
                     `agent ui show \"{}\"` lists it",
                    dialog.surface.name(),
                    dialog.surface.name()
                ),
                None => format!(
                    "{} is up, and it is not the agent's to answer",
                    super::policy::game_dialog(modal.id.value())
                        .unwrap_or("a dialog the agent did not open")
                ),
            });
        }
        if !entry.in_view {
            if !entry.scrolls {
                return Aim::Refused(format!(
                    "{} is outside what its window shows, and nothing scrolls it into view",
                    entry.path
                ));
            }
            if self.scrolls >= 2 {
                return Aim::Refused(format!("{} would not scroll into view", entry.path));
            }
            frame
                .events
                .push(request(entry.node, Action::ScrollIntoView, None));
            self.scrolls += 1;
            // The area moves on the pass that takes the request; the tree
            // shows it moved on the one after.
            self.wait = 1;
            return Aim::Wait;
        }
        let at = entry.rect.center();
        match frame.ctx.layer_id_at(at) {
            Some(layer) if target.owns(layer) => Aim::Ready,
            Some(layer) => {
                let over = tree.surfaces.iter().find(|s| s.owns(layer));
                // A notice drawn over a control does not stop a request
                // aimed at the control itself - only a pointer.
                if !pointer && over.is_some_and(|s| s.surface == Surface::Toasts) {
                    return Aim::Ready;
                }
                if !self.raised_front && matches!(target.surface, Surface::Window(_)) {
                    raise(frame.ctx, target.node);
                    self.raised_front = true;
                    self.wait = 1;
                    return Aim::Wait;
                }
                Aim::Refused(format!(
                    "{} is covered by {}",
                    entry.path,
                    over.map_or_else(
                        || "something the agent may not read".to_owned(),
                        |s| s.surface.name()
                    )
                ))
            }
            None => Aim::Refused(format!("nothing is drawn where {} is", entry.path)),
        }
    }

    /// Note what is up before an action, to tell afterwards what it raised.
    fn remember_before(&mut self, tree: &UiTree) {
        self.roots_before = tree.roots.clone();
        self.toasts_before = toasts(tree);
    }

    fn click(&mut self, tree: &UiTree, update: &TreeUpdate, mut frame: Frame, path: &str) -> Poll {
        if let Some((clicked, window)) = self.target.clone() {
            let answer = json!({ "clicked": clicked });
            return Poll::Done(self.after(tree, update, &mut frame, &clicked, &window, answer));
        }
        let entry = match tree.resolve(path) {
            Ok(entry) => entry.clone(),
            Err(e) => return Poll::Done(Err(e)),
        };
        if let Err(e) = usable(&entry) {
            return Poll::Done(Err(e));
        }
        let pointer = match entry.kind {
            Kind::Row => true,
            Kind::Button | Kind::Checkbox | Kind::Radio | Kind::Colour | Kind::Image
                if entry.clickable =>
            {
                false
            }
            Kind::ComboBox => {
                return Poll::Done(Err(format!(
                    "{} is a combo box: `agent ui choose \"{}\" <option>` picks from it",
                    entry.path, entry.path
                )));
            }
            Kind::TextField => {
                return Poll::Done(Err(format!(
                    "{} is a text field: `agent ui type \"{}\" <text>` fills it",
                    entry.path, entry.path
                )));
            }
            Kind::Slider | Kind::Number => {
                return Poll::Done(Err(format!(
                    "{} is a number: `agent ui set \"{}\" <value>` sets it",
                    entry.path, entry.path
                )));
            }
            _ => {
                return Poll::Done(Err(format!(
                    "{} is {} and nothing happens when it is clicked",
                    entry.path,
                    words_for(entry.kind)
                )));
            }
        };
        match self.aim(tree, &mut frame, &entry, pointer) {
            Aim::Ready => {}
            Aim::Wait => return Poll::Pending,
            Aim::Refused(why) => return Poll::Done(Err(why)),
        }
        self.remember_before(tree);
        if pointer {
            let at = entry.rect.center();
            frame.events.push(egui::Event::PointerMoved(at));
            frame.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            self.release_at = Some(at);
        } else {
            frame.events.push(request(entry.node, Action::Click, None));
            self.wait = SETTLE_FRAMES;
        }
        let window = tree.surfaces[entry.surface].surface.window().to_owned();
        self.target = Some((entry.path.clone(), window));
        Poll::Pending
    }

    fn type_into(
        &mut self,
        tree: &UiTree,
        mut frame: Frame,
        path: &str,
        text: &str,
        enter: bool,
    ) -> Poll {
        let entry = match tree.resolve(path) {
            Ok(entry) => entry.clone(),
            Err(_) if self.released => {
                // The field went with what Enter did - a dialog it closed.
                return Poll::Done(Ok(json!({ "typed": path, "value": self.typed_value })));
            }
            Err(e) => return Poll::Done(Err(e)),
        };
        if self.released {
            let mut answer = json!({ "typed": entry.path, "value": self.typed_value });
            if let Some(dialog) = raised_dialog(tree, frame.raised) {
                answer["dialog"] = dialog;
            }
            let new = new_toasts(&self.toasts_before, &toasts(tree));
            if !new.is_empty() {
                answer["toasts"] = json!(new);
            }
            return Poll::Done(Ok(answer));
        }
        if self.typed {
            self.typed_value = entry.value.clone();
            release_keyboard(frame.ctx);
            self.released = true;
            self.wait = 1;
            return Poll::Pending;
        }
        if self.delivered {
            if !entry.focused {
                if self.focus_tries >= 2 {
                    return Poll::Done(Err(format!("{} would not take the keyboard", entry.path)));
                }
                frame.events.push(request(entry.node, Action::Focus, None));
                self.focus_tries += 1;
                self.wait = 1;
                return Poll::Pending;
            }
            self.remember_before(tree);
            // Select what is there, so the text replaces it, as a person
            // retyping a field would.
            for pressed in [true, false] {
                frame
                    .events
                    .push(key(egui::Key::A, pressed, egui::Modifiers::COMMAND));
            }
            frame.events.push(egui::Event::Text(text.to_owned()));
            if enter {
                for pressed in [true, false] {
                    frame
                        .events
                        .push(key(egui::Key::Enter, pressed, egui::Modifiers::NONE));
                }
            }
            self.typed = true;
            self.wait = 1;
            return Poll::Pending;
        }
        if let Err(e) = usable(&entry) {
            return Poll::Done(Err(e));
        }
        if entry.kind != Kind::TextField || !entry.focusable {
            return Poll::Done(Err(format!(
                "{} is {}, not a text field",
                entry.path,
                words_for(entry.kind)
            )));
        }
        if entry.password {
            return Poll::Done(Err(format!(
                "{} takes a password, which the agent does not type",
                entry.path
            )));
        }
        if enter && entry.multiline {
            return Poll::Done(Err(format!(
                "Enter starts a new line in {}; the button beside it applies it",
                entry.path
            )));
        }
        match self.aim(tree, &mut frame, &entry, false) {
            Aim::Ready => {}
            Aim::Wait => return Poll::Pending,
            Aim::Refused(why) => return Poll::Done(Err(why)),
        }
        frame.events.push(request(entry.node, Action::Focus, None));
        self.focus_tries = 1;
        self.delivered = true;
        self.wait = 1;
        Poll::Pending
    }

    fn set(&mut self, tree: &UiTree, mut frame: Frame, path: &str, value: f64) -> Poll {
        let entry = match tree.resolve(path) {
            Ok(entry) => entry.clone(),
            Err(e) => return Poll::Done(Err(e)),
        };
        if self.delivered {
            return Poll::Done(Ok(json!({
                "set": entry.path,
                "asked": value,
                "number": entry.number,
                "shown": entry.value,
            })));
        }
        if let Err(e) = usable(&entry) {
            return Poll::Done(Err(e));
        }
        if !matches!(entry.kind, Kind::Slider | Kind::Number) || !entry.settable {
            return Poll::Done(Err(format!(
                "{} is {}, not a slider or a number",
                entry.path,
                words_for(entry.kind)
            )));
        }
        if !value.is_finite() {
            return Poll::Done(Err("the value must be a finite number".to_owned()));
        }
        match self.aim(tree, &mut frame, &entry, false) {
            Aim::Ready => {}
            Aim::Wait => return Poll::Pending,
            Aim::Refused(why) => return Poll::Done(Err(why)),
        }
        frame.events.push(request(
            entry.node,
            Action::SetValue,
            Some(ActionData::NumericValue(value)),
        ));
        self.delivered = true;
        self.wait = SETTLE_FRAMES;
        Poll::Pending
    }

    fn choose(
        &mut self,
        tree: &UiTree,
        update: &TreeUpdate,
        mut frame: Frame,
        path: &str,
        option: &str,
    ) -> Poll {
        if let Some(chosen) = self.picked.clone() {
            close_menus(frame.ctx, frame.raised);
            let value = tree.resolve(path).ok().and_then(|e| e.value.clone());
            return Poll::Done(Ok(json!({ "chose": chosen, "in": path, "value": value })));
        }
        let entry = match tree.resolve(path) {
            Ok(entry) => entry.clone(),
            Err(e) => {
                close_menus(frame.ctx, frame.raised);
                return Poll::Done(Err(e));
            }
        };
        if let Some(opened) = self.opened_at {
            let from = tree.surfaces[entry.surface].surface.window().to_owned();
            let menu = new_roots(&self.roots_before, update)
                .into_iter()
                .find(|(_, is_menu)| *is_menu)
                .map(|(node, _)| node);
            let Some(menu) = menu else {
                if self.frame > opened + 4 {
                    return Poll::Done(Err(format!(
                        "{} opened no list to choose from",
                        entry.path
                    )));
                }
                return Poll::Pending;
            };
            frame.raised.insert(menu, Surface::Menu { from });
            let Some(tree) = UiTree::read(update, frame.ctx.content_rect(), frame.raised) else {
                return Poll::Pending;
            };
            let Some(index) = tree.surfaces.iter().position(|s| s.node == menu) else {
                return Poll::Pending;
            };
            let options: Vec<&Entry> = tree
                .entries_of(index)
                .filter(|e| matches!(e.kind, Kind::Button | Kind::Checkbox | Kind::Radio))
                .collect();
            let pick = options
                .iter()
                .find(|e| e.label.as_deref() == Some(option))
                .or_else(|| {
                    let wanted = option.to_lowercase();
                    let mut matching = options.iter().filter(|e| {
                        e.label
                            .as_deref()
                            .is_some_and(|l| l.to_lowercase() == wanted)
                    });
                    match (matching.next(), matching.next()) {
                        (Some(one), None) => Some(one),
                        _ => None,
                    }
                })
                .copied()
                .cloned();
            let Some(pick) = pick else {
                let names: Vec<&str> = options.iter().filter_map(|e| e.label.as_deref()).collect();
                close_menus(frame.ctx, frame.raised);
                return Poll::Done(Err(format!(
                    "{} has no option {option:?}; it has: {}",
                    entry.path,
                    names.join(" | ")
                )));
            };
            if let Some(refused) = pick.refused {
                close_menus(frame.ctx, frame.raised);
                return Poll::Done(Err(refused.sentence(&pick.path)));
            }
            if !pick.enabled {
                close_menus(frame.ctx, frame.raised);
                return Poll::Done(Err(format!("{} is greyed out", pick.path)));
            }
            frame.events.push(request(pick.node, Action::Click, None));
            self.picked = pick.label.clone().or(Some(option.to_owned()));
            self.wait = SETTLE_FRAMES;
            return Poll::Pending;
        }
        if let Err(e) = usable(&entry) {
            return Poll::Done(Err(e));
        }
        if !matches!(entry.kind, Kind::ComboBox | Kind::Button) || !entry.clickable {
            return Poll::Done(Err(format!(
                "{} is {}, which opens nothing to choose from",
                entry.path,
                words_for(entry.kind)
            )));
        }
        // A menu left open would take the choice's place.
        close_menus(frame.ctx, frame.raised);
        match self.aim(tree, &mut frame, &entry, false) {
            Aim::Ready => {}
            Aim::Wait => return Poll::Pending,
            Aim::Refused(why) => return Poll::Done(Err(why)),
        }
        self.remember_before(tree);
        frame.events.push(request(entry.node, Action::Click, None));
        self.opened_at = Some(self.frame);
        self.wait = 1;
        Poll::Pending
    }

    fn scroll(&mut self, tree: &UiTree, frame: Frame, window: &str, points: f32) -> Poll {
        let Some(index) = tree.surface_named(window) else {
            return Poll::Done(Err(not_open(tree, window)));
        };
        if self.delivered {
            let probe = tree.entries_of(index).find(|e| e.scrolls).map(|e| e.rect);
            // egui eases a scroll in, a pass or two late, and out: done
            // once the area has moved and come to rest - or, for a list
            // already at its end, once it plainly will not move.
            let still = probe.is_some() && probe == self.probe;
            if self.probe.is_some() && !still {
                self.moved = true;
            }
            self.probe = probe;
            let rested = self.moved && still;
            if !rested && self.frame < self.delivered_at + SCROLL_SETTLE_FRAMES {
                return Poll::Pending;
            }
            let mut listing = tree.surface_json(index);
            listing["scrolled"] = json!(points);
            return Poll::Done(Ok(listing));
        }
        let Some(inside) = tree.entries_of(index).find(|e| e.scrolls) else {
            return Poll::Done(Err(format!("nothing in {window} scrolls")));
        };
        if !points.is_finite() || points == 0.0 {
            return Poll::Done(Err(
                "scroll by a number of points: positive goes down".to_owned()
            ));
        }
        let steps = (points.abs() / SCROLL_STEP_POINTS).round().clamp(1.0, 30.0) as u32;
        let action = if points > 0.0 {
            Action::ScrollDown
        } else {
            Action::ScrollUp
        };
        for _ in 0..steps {
            frame.events.push(request(inside.node, action, None));
        }
        self.delivered = true;
        self.delivered_at = self.frame;
        Poll::Pending
    }

    /// What a click did: the control now, and a dialog, a menu or a notice
    /// it raised. A menu a plain click opened is closed again - it would
    /// hold the keyboard - and its options are listed for `choose`.
    #[allow(clippy::too_many_arguments)]
    fn after(
        &mut self,
        tree: &UiTree,
        update: &TreeUpdate,
        frame: &mut Frame,
        path: &str,
        from: &str,
        mut answer: Value,
    ) -> Result<Value, String> {
        let mut raised_any = false;
        for (node, is_menu) in new_roots(&self.roots_before, update) {
            let surface = if is_menu {
                Surface::Menu {
                    from: from.to_owned(),
                }
            } else {
                Surface::Dialog {
                    from: from.to_owned(),
                }
            };
            frame.raised.insert(node, surface);
            raised_any = true;
        }
        let fresh;
        let tree = if raised_any {
            fresh = UiTree::read(update, frame.ctx.content_rect(), frame.raised)
                .unwrap_or_else(|| tree.clone());
            &fresh
        } else {
            tree
        };
        if let Ok(entry) = tree.resolve(path) {
            answer["now"] = entry.to_json();
        }
        if let Some(dialog) = raised_dialog(tree, frame.raised) {
            answer["dialog"] = dialog;
        }
        let menus: Vec<Value> = tree
            .surfaces
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.surface, Surface::Menu { .. }))
            .map(|(index, _)| {
                let options: Vec<&str> = tree
                    .entries_of(index)
                    .filter_map(|e| e.label.as_deref())
                    .collect();
                json!(options)
            })
            .collect();
        if !menus.is_empty() {
            close_menus(frame.ctx, frame.raised);
            answer["menu"] = json!({
                "options": menus.into_iter().next(),
                "closed": true,
                "note": "a menu holds the keyboard, so it was closed again; `agent ui choose` \
                         picks from it",
            });
        }
        let new = new_toasts(&self.toasts_before, &toasts(tree));
        if !new.is_empty() {
            answer["toasts"] = json!(new);
        }
        Ok(answer)
    }
}

/// Refused, or greyed out: nothing to act on.
fn usable(entry: &Entry) -> Result<(), String> {
    if let Some(refused) = entry.refused {
        return Err(refused.sentence(&entry.path));
    }
    if entry.is_words() {
        return Err(format!("{} is words, not a control", entry.path));
    }
    if !entry.enabled {
        return Err(format!("{} is greyed out", entry.path));
    }
    Ok(())
}

fn words_for(kind: Kind) -> String {
    let word = kind.word();
    let article = if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {word}")
}

fn request(node: NodeId, action: Action, data: Option<ActionData>) -> egui::Event {
    egui::Event::AccessKitActionRequest(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: node,
        data,
    })
}

fn key(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    }
}

/// Bring the window whose node is `node` to the front, as a person's click
/// on it would.
fn raise(ctx: &egui::Context, node: NodeId) {
    let layer = ctx
        .memory(|m| m.areas().visible_layer_ids())
        .into_iter()
        .find(|layer| layer.id.with("move").value() == node.0);
    if let Some(layer) = layer {
        ctx.move_to_top(layer);
    }
}

/// Take the keyboard back from whatever field holds it.
pub(super) fn release_keyboard(ctx: &egui::Context) {
    ctx.memory_mut(|m| {
        if let Some(focused) = m.focused() {
            m.surrender_focus(focused);
        }
        m.stop_text_input();
    });
}

/// Close every open menu, and forget the ones the agent opened.
pub(super) fn close_menus(ctx: &egui::Context, raised: &mut HashMap<NodeId, Surface>) {
    egui::Popup::close_all(ctx);
    raised.retain(|_, surface| !matches!(surface, Surface::Menu { .. }));
}

/// The nodes under the root that were not there before, and whether each
/// is a menu - a clickable area of its own - rather than a dialog. Read by
/// role and bounds only: whose they are decides whether their words are.
fn new_roots(before: &[NodeId], update: &TreeUpdate) -> Vec<(NodeId, bool)> {
    let Some(root) = update.tree.as_ref().map(|t| t.root) else {
        return Vec::new();
    };
    let Some((_, root)) = update.nodes.iter().find(|(id, _)| *id == root) else {
        return Vec::new();
    };
    let toasts = NodeId(egui::Id::new(super::tree::TOASTS_AREA).value());
    root.children()
        .iter()
        .copied()
        // The game's own dialogs are never the agent's, whatever it
        // clicked just before one came up.
        .filter(|child| {
            !before.contains(child)
                && *child != toasts
                && super::policy::game_dialog(child.0).is_none()
        })
        .filter_map(|child| {
            let (_, node) = update.nodes.iter().find(|(id, _)| *id == child)?;
            match node.role() {
                Role::Window => None,
                Role::Unknown if node.bounds().is_some() => Some((child, true)),
                _ => Some((child, false)),
            }
        })
        .collect()
}

/// Why a picture of the interface may not be taken now: it would show the
/// pixels of what the listing will not read - a window holding other
/// players' words, a dialog or a menu the agent did not raise.
fn picture_blocker(
    tree: &UiTree,
    update: &TreeUpdate,
    ctx: &egui::Context,
    raised: &HashMap<NodeId, Surface>,
) -> Option<String> {
    if let Some(title) = tree
        .refused_windows
        .iter()
        .find(|title| super::policy::holds_others_words(title))
    {
        return Some(if title == "Gateway" {
            "the gateway picker is open, and a picture would show what it holds; walking out \
             of the gateway closes it"
                .to_owned()
        } else {
            format!(
                "the {title} window is open, and a picture would show what it holds; `agent ui \
                 close {title}` closes it"
            )
        });
    }
    if let Some(modal) = ctx.memory(|m| m.top_modal_layer())
        && !tree.surfaces.iter().any(|s| s.owns(modal))
    {
        return Some(format!(
            "{} is up, and a picture would show it",
            super::policy::game_dialog(modal.id.value())
                .unwrap_or("a dialog the agent did not open")
        ));
    }
    let stray_menu = new_roots(&[], update)
        .into_iter()
        .any(|(node, is_menu)| is_menu && !raised.contains_key(&node));
    stray_menu
        .then(|| "a menu the agent did not open is up, and a picture would show it".to_owned())
}

/// The toasts' words, in order.
fn toasts(tree: &UiTree) -> Vec<String> {
    tree.surfaces
        .iter()
        .position(|s| s.surface == Surface::Toasts)
        .map(|index| {
            tree.entries_of(index)
                .filter(|e| e.kind == Kind::Text)
                .filter_map(|e| e.value.clone())
                // A toast's icon is a label of its own.
                .filter(|text| text.chars().any(char::is_alphanumeric))
                .collect()
        })
        .unwrap_or_default()
}

/// The toasts in `now` that were not in `before`.
fn new_toasts(before: &[String], now: &[String]) -> Vec<String> {
    now.iter()
        .filter(|t| !before.contains(t))
        .cloned()
        .collect()
}

/// The listing of a dialog the agent's clicks raised, if one is up.
fn raised_dialog(tree: &UiTree, raised: &HashMap<NodeId, Surface>) -> Option<Value> {
    tree.surfaces
        .iter()
        .position(|s| matches!(s.surface, Surface::Dialog { .. }) && raised.contains_key(&s.node))
        .map(|index| tree.surface_json(index))
}

fn not_open(tree: &UiTree, name: &str) -> String {
    if let Some(title) = tree
        .refused_windows
        .iter()
        .find(|t| t.to_lowercase() == name.trim().to_lowercase())
    {
        return super::policy::window_refusal(title).sentence(title);
    }
    // A closed window the agent may not open is refused as such, rather
    // than pointed at a `ui open` that would refuse it.
    if let Some(window) = super::policy::window_named(name)
        && let Some(refusal) = super::policy::open_refusal(window)
    {
        return refusal.sentence(crate::ui::shortcuts::window_title(window));
    }
    format!("{name} is not open; `agent ui open \"{name}\"` opens it")
}

/// One surface's listing.
fn show(tree: &UiTree, name: &str) -> Result<Value, String> {
    tree.surface_named(name)
        .map(|index| tree.surface_json(index))
        .ok_or_else(|| not_open(tree, name))
}

/// What is open: the readable windows with how much each shows, a dialog
/// or a menu, the focused field, the toasts - and the open windows the
/// agent may not read, by title, with why.
fn summary(tree: &UiTree, ctx: &egui::Context) -> Value {
    let windows: Vec<Value> = tree
        .surfaces
        .iter()
        .enumerate()
        .filter(|(_, s)| matches!(s.surface, Surface::Window(_)))
        .map(|(index, s)| {
            let entries: Vec<&Entry> = tree.entries_of(index).collect();
            json!({
                "window": s.surface.name(),
                "entries": entries.len(),
                "out_of_view": entries.iter().filter(|e| !e.in_view).count(),
            })
        })
        .collect();
    let dialog = tree
        .surfaces
        .iter()
        .position(|s| matches!(s.surface, Surface::Dialog { .. }))
        .map(|index| tree.surface_json(index))
        .or_else(|| {
            ctx.memory(|m| m.top_modal_layer()).map(|layer| {
                let what = super::policy::game_dialog(layer.id.value())
                    .unwrap_or("a dialog the agent did not open");
                json!({
                    "dialog": what,
                    "refused": "it is not the agent's to read or answer",
                })
            })
        });
    let not_readable: Vec<Value> = tree
        .refused_windows
        .iter()
        .map(|title| {
            let refusal = super::policy::window_refusal(title);
            json!({ "window": title, "why": refusal.why, "instead": refusal.instead })
        })
        .collect();
    json!({
        "windows": windows,
        "dialog": dialog,
        "focused": tree.entries.iter().find(|e| e.focused).map(|e| e.path.clone()),
        "toasts": toasts(tree),
        "not_readable": not_readable,
    })
}
