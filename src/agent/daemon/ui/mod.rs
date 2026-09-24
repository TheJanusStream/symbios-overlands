//! `agent ui` (#1424): the game's own interface, read and worked as a
//! person reads and works it - window by window, each control named by
//! where it is drawn.
//!
//! The semantic commands cover what the agent mostly does; this is the long
//! tail - any setting, any editor field, any button in the windows it may
//! use. `ui` says what is open; `ui show <window>` lists a window's controls
//! and words, each by the path the other commands take (`Avatar > hair >
//! width`); `ui open`/`ui close` work the toolbar's toggles; `ui click`,
//! `ui type`, `ui set`, `ui choose` and `ui scroll` work the controls.
//!
//! **What it may read and do** is [`policy`]: an allow-list of windows,
//! read through [`tree`] so that nothing outside it is ever copied - Chat
//! and Diagnostics carry other players' words, the gateway picker their
//! display names - and inside it, a refusal list of the controls whose work
//! has a gate elsewhere: every Save (`--allow-save`), Visit (the
//! unsaved-edits rule), muting, the operator's clipboard and browser.
//! egui's own requests to copy or open a link are dropped before they reach
//! the desktop ([`keep_off_the_desktop`]), for the hosted editors whose code
//! is not in this repo.
//!
//! **How it acts** is [`drive`]: AccessKit requests aimed at one widget
//! each, after the checks that keep a person's hand off a control - a
//! dialog over it, its being out of view or under another window.
//!
//! **What it costs.** AccessKit is on only while a command is at work:
//! turned on when one arrives, off when it answers - egui builds nothing
//! for it otherwise (#1424, measured: always-on cost half a point of a core
//! at idle). A window left open costs its drawing every frame whether
//! anything reads it or not - the World Editor about two points - so
//! `status.interface` names the open ones, and `ui close` closes them.
//!
//! **What it checks every time.** Each answer to a command that could
//! change a record says which of the world, avatar and inventory records
//! changed while it ran, bit for bit (`changed`), and which were written
//! without changing (`touched`) - a drawn panel has rewritten a record with
//! nobody touching it before (#1390), and opening a window is the one way
//! the agent can cause that.

mod drive;
mod picture;
mod policy;
mod tree;

use std::collections::HashMap;
use std::sync::mpsc;

use bevy::ecs::change_detection::Tick;
use bevy::prelude::*;
use bevy_egui::egui::accesskit::NodeId;
use bevy_egui::{EguiContext, EguiFullOutput, EguiInput, EguiOutput, PrimaryEguiContext, egui};
use serde_json::{Value, json};

use crate::state::{AppState, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord, TravelingTo};
use crate::ui::layout::UiWindow;
use crate::ui::shortcuts::window_title;
use crate::ui::toolbar::UiPanels;

use super::super::control::protocol::{Response, UiRequest};
use drive::{Driver, Frame, Poll, Verb};
use tree::Surface;

/// A command at work, over as many frames as it takes.
#[derive(Resource)]
pub(super) struct UiWork {
    driver: Driver,
    reply: mpsc::Sender<Response>,
    before: Records,
    /// Whether the answer comes with a picture of what it lists.
    wants_picture: bool,
    /// The picture being taken, once the listing is in.
    picture: Option<picture::Picturing>,
}

/// The dialogs and menus the agent's own clicks raised and that are still
/// up, by their node under the tree's root - the ones it may read.
#[derive(Resource, Default)]
pub(super) struct UiRaised(HashMap<NodeId, Surface>);

/// Whether an interface command that may write a record is at work: the
/// daemon answers nothing else that writes one until it has (`serve`).
pub(super) fn acting(world: &World) -> bool {
    world
        .get_resource::<UiWork>()
        .is_some_and(|work| work.driver.verb().acts())
}

/// Start an interface command; its answer goes to `reply` once it has run.
pub(super) fn begin(world: &mut World, request: UiRequest, reply: mpsc::Sender<Response>) {
    if let Err(why) = start(world, request, &reply) {
        let _ = reply.send(Response::failure(why));
    }
}

fn start(
    world: &mut World,
    request: UiRequest,
    reply: &mpsc::Sender<Response>,
) -> Result<(), String> {
    if world.contains_resource::<UiWork>() {
        return Err(
            "the agent is already working the interface; ask again when that answers".to_owned(),
        );
    }
    if *world.resource::<State<AppState>>().get() != AppState::InGame {
        return Err("the agent is not in a world yet".to_owned());
    }
    if world.contains_resource::<TravelingTo>() {
        return Err("the agent is travelling".to_owned());
    }
    let ctx = primary_context(world).ok_or("the interface is not up yet")?;
    let wants_picture = matches!(
        request,
        UiRequest::Summary { picture: true } | UiRequest::Show { picture: true, .. }
    );
    let verb = match request {
        UiRequest::Summary { .. } => Verb::Summary,
        UiRequest::Show { window, .. } => Verb::Show(window),
        UiRequest::Open { window } => open(world, &ctx, &window)?,
        UiRequest::Close { window } => close(world, &window)?,
        UiRequest::Click { path } => Verb::Click(path),
        UiRequest::Type { path, text, enter } => Verb::Type { path, text, enter },
        UiRequest::Set { path, value } => Verb::Set { path, value },
        UiRequest::Choose { path, option } => Verb::Choose { path, option },
        UiRequest::Scroll { window, points } => Verb::Scroll { window, points },
    };
    ctx.enable_accesskit();
    let before = Records::read(world);
    world.insert_resource(UiWork {
        driver: Driver::new(verb),
        reply: reply.clone(),
        before,
        wants_picture,
        picture: None,
    });
    Ok(())
}

/// `ui open`: the toolbar's own toggle, for a window the agent may use.
fn open(world: &mut World, ctx: &egui::Context, name: &str) -> Result<Verb, String> {
    let window = named(name)?;
    if let Some(refusal) = policy::open_refusal(window) {
        return Err(refusal.sentence(window_title(window)));
    }
    if window == UiWindow::AudioEditor {
        return Err(
            "the Audio Editor has no toolbar button: it opens from inside an editor, for the \
             sound it edits - `agent ui click` that sound's Edit audio button"
                .to_owned(),
        );
    }
    if window == UiWindow::WorldEditor && !super::edit::owns_room(world) {
        return Err(
            "the World Editor opens in the agent's own world only, as its toolbar button does; \
             `agent travel home` goes back to it"
                .to_owned(),
        );
    }
    set_panel(world, window, true);
    // A window collapsed to its title bar draws nothing else.
    crate::ui::shortcuts::expand_window(ctx, window);
    Ok(Verb::Open(window_title(window).to_owned()))
}

/// `ui close`: any toolbar window - closing reads nothing, so the refused
/// ones too - or the audio pop-out, by its own close button.
fn close(world: &mut World, name: &str) -> Result<Verb, String> {
    if name.trim().to_lowercase().starts_with("audio editor") {
        return Ok(Verb::CloseByButton(name.trim().to_owned()));
    }
    let window = named(name)?;
    set_panel(world, window, false);
    Ok(Verb::Closed(window_title(window).to_owned()))
}

/// The toolbar window `name` stands for.
fn named(name: &str) -> Result<UiWindow, String> {
    policy::window_named(name).ok_or_else(|| {
        if name.trim().eq_ignore_ascii_case("gateway") {
            return policy::window_refusal("Gateway").sentence("The gateway picker");
        }
        let windows: Vec<&str> = policy::WINDOWS.iter().map(|w| window_title(*w)).collect();
        format!(
            "no window is called {name:?}; the agent's are {}",
            windows.join(", ")
        )
    })
}

/// Open or close `window` as its toolbar button does, touching the panels
/// only when that changes something: a change is saved to the agent's
/// settings, and a borrow alone would count as one.
fn set_panel(world: &mut World, window: UiWindow, open: bool) {
    let shown = |panels: &UiPanels| match window {
        UiWindow::Chat => panels.chat,
        UiWindow::People => panels.people,
        UiWindow::Avatar => panels.avatar,
        UiWindow::Inventory => panels.inventory,
        UiWindow::Catalogue => panels.catalogue,
        UiWindow::WorldEditor => panels.world_editor,
        UiWindow::Diagnostics => panels.diagnostics,
        UiWindow::Settings => panels.settings,
        UiWindow::Controls => panels.controls,
        UiWindow::AudioEditor => open,
    };
    if shown(world.resource::<UiPanels>()) == open {
        return;
    }
    let mut panels = world.resource_mut::<UiPanels>();
    let flag = match window {
        UiWindow::Chat => &mut panels.chat,
        UiWindow::People => &mut panels.people,
        UiWindow::Avatar => &mut panels.avatar,
        UiWindow::Inventory => &mut panels.inventory,
        UiWindow::Catalogue => &mut panels.catalogue,
        UiWindow::WorldEditor => &mut panels.world_editor,
        UiWindow::Diagnostics => &mut panels.diagnostics,
        UiWindow::Settings => &mut panels.settings,
        UiWindow::Controls => &mut panels.controls,
        UiWindow::AudioEditor => return,
    };
    *flag = open;
}

/// Work the command at hand one frame further. After `serve`, in `Update`:
/// what it sends egui is taken by this frame's pass, in `PostUpdate`.
pub(super) fn advance(world: &mut World) {
    let Some(mut work) = world.remove_resource::<UiWork>() else {
        return;
    };
    if work.picture.is_some() {
        match picture::advance(world, &mut work) {
            picture::Step::Pending => world.insert_resource(work),
            picture::Step::Done(result) => finish(world, work, result),
        }
        return;
    }
    let mut raised = world.remove_resource::<UiRaised>().unwrap_or_default();
    let mut pixels_per_point = 1.0;
    let poll = {
        let mut contexts = world.query_filtered::<
            (&mut EguiContext, &EguiOutput, &mut EguiInput),
            With<PrimaryEguiContext>,
        >();
        match contexts.single_mut(world) {
            Ok((mut context, output, mut input)) => {
                let ctx = context.get_mut().clone();
                pixels_per_point = ctx.pixels_per_point();
                work.driver.step(Frame {
                    ctx: &ctx,
                    tree: output.platform_output.accesskit_update.as_ref(),
                    events: &mut input.0.events,
                    raised: &mut raised.0,
                })
            }
            Err(_) => Poll::Done(Err("the interface went away".to_owned())),
        }
    };
    match poll {
        Poll::Pending => {
            world.insert_resource(raised);
            world.insert_resource(work);
        }
        Poll::Done(result) => {
            if let Some(ctx) = primary_context(world) {
                // Whatever happened, nothing is left holding the keyboard:
                // a command given up half way can have focused a field or
                // opened a menu, and either stops the agent walking.
                drive::release_keyboard(&ctx);
                drive::close_menus(&ctx, &mut raised.0);
                ctx.disable_accesskit();
            }
            world.insert_resource(raised);
            match result {
                // The listing is in; the picture of it is taken next - not
                // while it would show what the listing does not read.
                Ok(_) if work.wants_picture && work.driver.picture_blocker().is_some() => {
                    let why = work.driver.picture_blocker().unwrap_or_default().to_owned();
                    finish(world, work, Err(format!("no picture: {why}")));
                }
                Ok(answer) if work.wants_picture => {
                    let crop = work.driver.shown_rect();
                    match picture::begin(world, answer, crop, pixels_per_point) {
                        Ok(picturing) => {
                            work.picture = Some(picturing);
                            world.insert_resource(work);
                        }
                        Err(why) => finish(world, work, Err(why)),
                    }
                }
                result => finish(world, work, result),
            }
        }
    }
}

fn finish(world: &mut World, work: UiWork, result: Result<Value, String>) {
    let response = match result {
        Ok(mut answer) => {
            let verb = work.driver.verb();
            if verb.acts() || matches!(verb, Verb::Open(_)) {
                let (changed, touched) = work.before.compare(&Records::read(world));
                answer["changed"] = json!(changed);
                answer["touched"] = json!(touched);
            }
            if matches!(verb, Verb::Summary) {
                answer["can_open"] = json!(can_open(world));
            }
            Response::success(answer)
        }
        Err(why) => Response::failure(why),
    };
    // INTENTIONAL: the client may have stopped waiting.
    let _ = work.reply.send(response);
}

/// The agent's windows that are closed and that it may open here.
fn can_open(world: &World) -> Vec<&'static str> {
    let own = super::edit::owns_room(world);
    let panels = world.resource::<UiPanels>();
    policy::WINDOWS
        .into_iter()
        .filter(|window| {
            let open = match window {
                UiWindow::People => panels.people,
                UiWindow::Avatar => panels.avatar,
                UiWindow::Inventory => panels.inventory,
                UiWindow::Catalogue => panels.catalogue,
                UiWindow::WorldEditor => panels.world_editor || !own,
                UiWindow::Settings => panels.settings,
                UiWindow::Controls => panels.controls,
                _ => true,
            };
            !open
        })
        .map(window_title)
        .collect()
}

/// What `status` says about the interface: which windows are open - each
/// costs its drawing every frame - and whether a command is at work.
pub(super) fn describe(world: &World) -> Value {
    let open: Vec<&str> = world
        .get_resource::<UiPanels>()
        .map_or_else(Vec::new, |panels| {
            [
                (panels.chat, UiWindow::Chat),
                (panels.people, UiWindow::People),
                (panels.avatar, UiWindow::Avatar),
                (panels.inventory, UiWindow::Inventory),
                (panels.catalogue, UiWindow::Catalogue),
                (panels.world_editor, UiWindow::WorldEditor),
                (panels.diagnostics, UiWindow::Diagnostics),
                (panels.settings, UiWindow::Settings),
                (panels.controls, UiWindow::Controls),
            ]
            .into_iter()
            .filter(|(shown, _)| *shown)
            .map(|(_, window)| window_title(window))
            .collect()
        });
    json!({
        "open": open,
        "working": world.contains_resource::<UiWork>(),
    })
}

/// The one egui context the game draws its interface in.
fn primary_context(world: &mut World) -> Option<egui::Context> {
    world
        .query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>()
        .single_mut(world)
        .ok()
        .map(|mut context| context.get_mut().clone())
}

/// The daemon runs on its operator's desktop and has nothing to hand them.
/// egui's own requests to write the clipboard or open a link - an editor's
/// "Copy JSON", a hosted crate's hyperlink - are dropped here, before
/// bevy_egui acts on them. The game's own copy buttons and links, which go
/// around egui, are on the refusal list ([`policy`]).
pub(super) fn keep_off_the_desktop(mut outputs: Query<&mut EguiFullOutput>) {
    let off = |command: &egui::OutputCommand| {
        matches!(
            command,
            egui::OutputCommand::CopyText(_)
                | egui::OutputCommand::CopyImage(_)
                | egui::OutputCommand::OpenUrl(_)
        )
    };
    for mut output in &mut outputs {
        let wants = output
            .0
            .as_ref()
            .is_some_and(|full| full.platform_output.commands.iter().any(off));
        if !wants {
            continue;
        }
        if let Some(full) = output.0.as_mut() {
            full.platform_output
                .commands
                .retain(|command| !off(command));
            info!(
                "An interface control asked to copy or to open a link; the agent's daemon keeps \
                 off its operator's desktop"
            );
        }
    }
}

/// The agent's three records as they stand: exactly (`Debug` prints every
/// float the shortest way that reads back to the same bits), and when each
/// was last written.
struct Records {
    room: Option<(String, Tick)>,
    avatar: Option<(String, Tick)>,
    inventory: Option<(String, Tick)>,
}

impl Records {
    fn read(world: &World) -> Self {
        fn one<R: Resource, T: std::fmt::Debug>(
            world: &World,
            inner: impl Fn(&R) -> &T,
        ) -> Option<(String, Tick)> {
            let record = world.get_resource_ref::<R>()?;
            Some((format!("{:?}", inner(&record)), record.last_changed()))
        }
        Self {
            room: one(world, |live: &LiveRoomRecord| &live.0),
            avatar: one(world, |live: &LiveAvatarRecord| &live.0),
            inventory: one(world, |live: &LiveInventoryRecord| &live.0),
        }
    }

    /// Which records differ in `after` - by value, and by a write that
    /// changed nothing.
    fn compare(&self, after: &Self) -> (Vec<&'static str>, Vec<&'static str>) {
        let mut changed = Vec::new();
        let mut touched = Vec::new();
        for (name, before, after) in [
            ("room", &self.room, &after.room),
            ("avatar", &self.avatar, &after.avatar),
            ("inventory", &self.inventory, &after.inventory),
        ] {
            match (before, after) {
                (Some((was, was_tick)), Some((now, now_tick))) => {
                    if was != now {
                        changed.push(name);
                    } else if was_tick != now_tick {
                        touched.push(name);
                    }
                }
                (None, None) => {}
                _ => changed.push(name),
            }
        }
        (changed, touched)
    }
}

/// A control being worked, as `serve`'s tests need one: the daemon answers
/// nothing else that writes a record until it is done.
#[cfg(test)]
pub(super) fn working_a_control(world: &mut World) {
    let (reply, _) = mpsc::channel();
    let before = Records::read(world);
    world.insert_resource(UiWork {
        driver: Driver::new(Verb::Click("Avatar > Re-roll".into())),
        reply,
        before,
        wants_picture: false,
        picture: None,
    });
}

#[cfg(test)]
mod tests;
