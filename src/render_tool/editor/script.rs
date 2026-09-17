//! `--editor-script`: a shot's gestures, played frame by frame.
//!
//! A picture of the editor is a sequence of ordinary gestures (point at a
//! row, drag a gizmo handle, click Undo), and a clip only reads as one if
//! every gesture lands on the frame it was written for. This module parses a
//! small step language and plays it on the tool's hand-driven clock: the
//! steps before `start` run while the scene warms up and are never captured,
//! and every step after it advances exactly one captured frame at a time.
//!
//! ```text
//! click widget "Placements" over 8  # glide onto a widget, rest, press, release
//! click right-of "Theme:"           # the control beside a label
//! type "253"                        # select all in the focused field, then type
//! start                             # everything above runs before frame 0
//! hold 10                           # ten frames of nothing
//! move gizmo x over 6               # onto the selected gizmo's X arrow
//! press
//! drag-gizmo x 6.0 over 20          # pull the held handle 6 m along X
//! release
//! move ground 118,-3 over 20        # onto the terrain at x,z
//! move px 640,360                   # onto a frame pixel (default: one frame)
//! ```
//!
//! **One pointer, four readers.** The tool has no mouse, so each frame's
//! pointer is written everywhere the game reads a real one: egui's input
//! events (windows, trees, buttons), bevy_picking's mouse pointer (the
//! transform gizmo's hover), the primary window's cursor position (the
//! gizmo's drag, the scene pick and the drop) and `MouseButtonInput` (the
//! gizmo's drag edges and the drop's release). [`paint_cursor`] draws an
//! arrow where the pointer is, above every window, so a clip shows what is
//! being pointed at.
//!
//! **Targets are found, not guessed.** A widget is named by the text AccessKit
//! reports for it, its label or the value of a text field or combo box, and a
//! name that matches nothing or more than one control stops the run and says
//! so. `right-of` takes the nearest control on a label's row, because the
//! padlocks beside Landform, Biome and Theme are all the same glyph. A gizmo
//! handle is the middle of that axis's translate arrow, placed in world space
//! the way the gizmo crate sizes the arrow, so a foreshortened axis is still
//! hit.

use bevy::camera::RenderTarget;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseButtonInput;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowRef};
use bevy_egui::egui::accesskit;
use bevy_egui::{EguiContext, EguiContexts, EguiOutput, PrimaryEguiContext, egui};
use transform_gizmo_bevy::{GizmoOptions, GizmoOrientation, GizmoTarget};

use crate::camera::IsWorldCamera;
use crate::terrain::FinishedHeightMap;

use super::super::headless::{ClipStarted, Clock};

/// Frames a click spends after its glide: one resting on the target, one
/// pressing, one releasing. The rest is not decoration: the scene pick
/// leaves a press on a gizmo handle alone only if the handle was already
/// hovered on the frame before.
const CLICK_TAIL: u32 = 3;

/// The glide a `click` takes when it names none.
const CLICK_GLIDE: u32 = 6;

/// Script frames a setup step waits for its target to appear: a window lays
/// itself out over a frame or two, and a new window's first frame is an
/// invisible sizing pass. A step after `start` does not wait, because every
/// frame it waited would be a captured frame.
const SETUP_PATIENCE: u32 = 120;

/// Where along a translate arrow `gizmo <axis>` points, in pixels from the
/// gizmo's centre. transform-gizmo draws its arrows from about 17 to 67 px at
/// its default 75 px size when rotation handles share the gizmo.
const HANDLE_PX: f32 = 42.0;

/// The painted cursor's size over its 20 px outline.
const CURSOR_SCALE: f32 = 1.4;

/// A gizmo axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "z" => Some(Self::Z),
            _ => None,
        }
    }

    fn unit(self) -> Vec3 {
        match self {
            Self::X => Vec3::X,
            Self::Y => Vec3::Y,
            Self::Z => Vec3::Z,
        }
    }
}

/// What a `move` or `click` points at.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    /// The one control whose AccessKit label or value is this text.
    Widget(String),
    /// The nearest control to the right of the label with this text.
    RightOf(String),
    /// A pixel of the rendered frame.
    Px(Vec2),
    /// The middle of the selected gizmo's translate arrow for this axis.
    Gizmo(Axis),
    /// The terrain surface at this `x,z`.
    Ground(Vec2),
}

/// One step of a script.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    Hold(u32),
    Move {
        to: Target,
        over: u32,
    },
    Press,
    Release,
    Click {
        on: Target,
        over: u32,
    },
    Type(String),
    /// Wheel the surface under the pointer, in egui points: positive scrolls
    /// DOWN a list, as a wheel pulled toward you does.
    ///
    /// Needed because a panel can be taller than its window, and a control
    /// below the fold is clipped rather than culled: egui lays it out and
    /// reports it to AccessKit anyway, so `widget "..."` finds it, resolves
    /// it to a point outside the window, and the click lands on whatever is
    /// behind. Scrolling is the only way to put it where a pointer can reach
    /// it. The Body tab's hosted sculpting sections are all below the fold at
    /// any window a 720-line frame can hold (#1358).
    Scroll(f32),
    DragGizmo {
        axis: Axis,
        metres: f32,
        over: u32,
    },
}

impl Step {
    /// Script frames this step occupies.
    pub(crate) fn frames(&self) -> u32 {
        match self {
            Self::Hold(n) => *n,
            Self::Move { over, .. } | Self::DragGizmo { over, .. } => *over,
            Self::Press | Self::Release | Self::Type(_) | Self::Scroll(_) => 1,
            Self::Click { over, .. } => over + CLICK_TAIL,
        }
    }
}

/// A parsed `--editor-script`.
#[derive(Resource, Clone, Debug, PartialEq)]
pub(crate) struct EditorScript {
    pub(crate) steps: Vec<Step>,
    /// How many of `steps` play before the first captured frame.
    pub(crate) start: usize,
}

impl EditorScript {
    /// Parse a script. Every error names its line.
    pub(crate) fn parse(source: &str) -> Result<Self, String> {
        let mut steps = Vec::new();
        let mut start = None;
        for (index, raw) in source.lines().enumerate() {
            let line = index + 1;
            let words = words(raw).map_err(|e| format!("line {line}: {e}"))?;
            let Some(first) = words.first() else {
                continue;
            };
            if first.is_word("start") {
                if words.len() != 1 {
                    return Err(format!("line {line}: `start` takes nothing"));
                }
                if start.replace(steps.len()).is_some() {
                    return Err(format!("line {line}: `start` appears twice"));
                }
                continue;
            }
            steps.push(step(&words).map_err(|e| format!("line {line}: {e}: {}", raw.trim()))?);
        }
        Ok(Self {
            start: start.unwrap_or(0),
            steps,
        })
    }
}

/// A word of a script line: bare, or a `"..."` run.
#[derive(Debug, PartialEq)]
enum Word {
    Bare(String),
    Quoted(String),
}

impl Word {
    fn is_word(&self, word: &str) -> bool {
        matches!(self, Self::Bare(bare) if bare == word)
    }

    fn bare(&self) -> Option<&str> {
        match self {
            Self::Bare(bare) => Some(bare),
            Self::Quoted(_) => None,
        }
    }

    fn quoted(&self) -> Option<&str> {
        match self {
            Self::Quoted(text) => Some(text),
            Self::Bare(_) => None,
        }
    }
}

/// Split a line into words, stopping at a `#` outside quotes.
fn words(line: &str) -> Result<Vec<Word>, String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '#' {
            break;
        } else if c == '"' {
            chars.next();
            let mut text = String::new();
            loop {
                match chars.next() {
                    Some('"') => break,
                    Some(ch) => text.push(ch),
                    None => return Err("a quote is never closed".into()),
                }
            }
            out.push(Word::Quoted(text));
        } else {
            let mut text = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() || ch == '"' || ch == '#' {
                    break;
                }
                text.push(ch);
                chars.next();
            }
            out.push(Word::Bare(text));
        }
    }
    Ok(out)
}

fn step(words: &[Word]) -> Result<Step, String> {
    let verb = words[0]
        .bare()
        .ok_or("a step starts with a verb, not a quoted string")?;
    let rest = &words[1..];
    match verb {
        "hold" => match rest {
            [n] => Ok(Step::Hold(frames(n)?)),
            _ => Err("expected `hold <frames>`".into()),
        },
        "press" if rest.is_empty() => Ok(Step::Press),
        "release" if rest.is_empty() => Ok(Step::Release),
        "press" | "release" => Err(format!("`{verb}` takes nothing")),
        "type" => match rest {
            [Word::Quoted(text)] => Ok(Step::Type(text.clone())),
            _ => Err("expected `type \"<text>\"`".into()),
        },
        "scroll" => match rest {
            [n] => {
                let by = n
                    .bare()
                    .and_then(|w| w.parse::<f32>().ok())
                    .filter(|v| v.is_finite() && *v != 0.0)
                    .ok_or("expected `scroll <points>`, a non-zero number")?;
                Ok(Step::Scroll(by))
            }
            _ => Err("expected `scroll <points>`".into()),
        },
        "move" | "click" => {
            let (target, rest) = target(rest)?;
            if verb == "move" {
                let over = over(rest)?.unwrap_or(1);
                Ok(Step::Move { to: target, over })
            } else {
                let over = over(rest)?.unwrap_or(CLICK_GLIDE);
                Ok(Step::Click { on: target, over })
            }
        }
        "drag-gizmo" => match rest {
            [axis, metres, rest @ ..] => {
                let axis = axis
                    .bare()
                    .and_then(Axis::parse)
                    .ok_or("expected an axis, x, y or z")?;
                let metres = metres
                    .bare()
                    .and_then(|m| m.parse::<f32>().ok())
                    .ok_or("expected a distance in metres")?;
                let over = over(rest)?.ok_or("expected `over <frames>`")?;
                Ok(Step::DragGizmo { axis, metres, over })
            }
            _ => Err("expected `drag-gizmo <x|y|z> <metres> over <frames>`".into()),
        },
        other => Err(format!(
            "unknown step `{other}`: expected hold, move, press, release, click, type, drag-gizmo or start"
        )),
    }
}

fn target(words: &[Word]) -> Result<(Target, &[Word]), String> {
    let (kind, rest) = words
        .split_first()
        .ok_or("expected a target: widget, right-of, px, gizmo or ground")?;
    let kind = kind
        .bare()
        .ok_or("expected a target kind before the quoted text")?;
    let (arg, rest) = rest
        .split_first()
        .ok_or_else(|| format!("`{kind}` needs an argument"))?;
    let target = match kind {
        "widget" => Target::Widget(
            arg.quoted()
                .ok_or("`widget` takes a quoted label")?
                .to_string(),
        ),
        "right-of" => Target::RightOf(
            arg.quoted()
                .ok_or("`right-of` takes a quoted label")?
                .to_string(),
        ),
        "px" => Target::Px(pair(arg).ok_or("`px` takes x,y")?),
        "gizmo" => Target::Gizmo(
            arg.bare()
                .and_then(Axis::parse)
                .ok_or("`gizmo` takes an axis, x, y or z")?,
        ),
        "ground" => Target::Ground(pair(arg).ok_or("`ground` takes x,z")?),
        other => {
            return Err(format!(
                "unknown target `{other}`: expected widget, right-of, px, gizmo or ground"
            ));
        }
    };
    Ok((target, rest))
}

fn pair(word: &Word) -> Option<Vec2> {
    let (a, b) = word.bare()?.split_once(',')?;
    Some(Vec2::new(a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn frames(word: &Word) -> Result<u32, String> {
    word.bare()
        .and_then(|w| w.parse::<u32>().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| "expected a frame count of at least 1".to_string())
}

fn over(words: &[Word]) -> Result<Option<u32>, String> {
    match words {
        [] => Ok(None),
        [keyword, n] if keyword.is_word("over") => Ok(Some(frames(n)?)),
        _ => Err("expected nothing more, or `over <frames>`".into()),
    }
}

/// One script frame's gestures, for the system that delivers them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FrameInput {
    /// Where the pointer is, in frame pixels, once it has appeared.
    pub(crate) pointer: Option<Vec2>,
    /// Whether it moved this frame.
    pub(crate) moved: bool,
    pub(crate) press: bool,
    pub(crate) release: bool,
    pub(crate) typed: Option<String>,
    /// Points to wheel the surface under the pointer by, down-positive.
    pub(crate) scrolled: Option<f32>,
}

/// What a lookup found, in frame pixels.
pub(crate) enum Lookup {
    Found(Vec2),
    /// Nothing matched; the near misses, for the error.
    Missing(Vec<String>),
    /// More than one control matched; what they were.
    Ambiguous(Vec<String>),
}

/// Where targets are this frame. The live implementation reads the last egui
/// pass's AccessKit tree, the gizmo target and the camera; tests supply a
/// fixed one.
pub(crate) trait Scene {
    fn widget(&self, text: &str) -> Lookup;
    fn right_of(&self, text: &str) -> Lookup;
    /// The world point in the middle of `axis`'s translate arrow on the
    /// current gizmo target, and the axis direction in world space.
    fn gizmo_handle(&self, axis: Axis) -> Option<(Vec3, Vec3)>;
    fn ground(&self, at: Vec2) -> Option<Vec3>;
    fn project(&self, world: Vec3) -> Option<Vec2>;
}

/// The path the pointer takes through the current step.
#[derive(Clone, Copy, Debug, Default)]
enum Path {
    #[default]
    Still,
    Screen {
        from: Vec2,
        to: Vec2,
    },
    /// A drag along a gizmo axis: interpolated in the world and projected
    /// per frame, holding the offset the pointer grabbed the handle at.
    World {
        from: Vec3,
        to: Vec3,
        grab: Vec2,
    },
}

/// A target that has not appeared yet, or a path to it.
enum Resolved {
    Ready(Path),
    NotYet(String),
}

/// Where the script is: the step, the frame within it, and the pointer.
#[derive(Resource, Debug, Default)]
pub(crate) struct ScriptRunner {
    step: usize,
    frame: u32,
    pointer: Option<Vec2>,
    path: Path,
    waited: u32,
}

impl ScriptRunner {
    /// Whether every step before `start` has played.
    pub(crate) fn setup_done(&self, script: &EditorScript) -> bool {
        self.step >= script.start
    }

    /// Play one script frame and say what it asks of the pointer.
    pub(crate) fn advance(
        &mut self,
        script: &EditorScript,
        scene: &dyn Scene,
    ) -> Result<FrameInput, String> {
        let mut input = FrameInput {
            pointer: self.pointer,
            ..default()
        };
        let Some(step) = script.steps.get(self.step) else {
            return Ok(input);
        };
        let in_setup = self.step < script.start;
        let frame = self.frame;
        match step {
            Step::Hold(_) => {}
            Step::Press => input.press = true,
            Step::Release => input.release = true,
            Step::Type(text) => input.typed = Some(text.clone()),
            Step::Scroll(by) => input.scrolled = Some(*by),
            Step::Move { to, over } | Step::Click { on: to, over } => {
                if frame == 0 {
                    match self.screen_path(to, scene)? {
                        Resolved::Ready(path) => self.path = path,
                        Resolved::NotYet(what) => return self.wait(in_setup, input, &what),
                    }
                }
                if frame < *over {
                    self.glide(frame, *over, scene, &mut input);
                } else if frame == over + 1 {
                    input.press = true;
                } else if frame == over + 2 {
                    input.release = true;
                }
            }
            Step::DragGizmo { axis, metres, over } => {
                if frame == 0 {
                    let Some((handle, dir)) = scene.gizmo_handle(*axis) else {
                        return self.wait(in_setup, input, "a gizmo to drag");
                    };
                    let at = scene
                        .project(handle)
                        .ok_or("the gizmo handle is off screen")?;
                    let pointer = self.pointer.ok_or(
                        "drag-gizmo needs the pointer on the handle first: `move gizmo <axis>`",
                    )?;
                    self.path = Path::World {
                        from: handle,
                        to: handle + dir * *metres,
                        grab: pointer - at,
                    };
                }
                self.glide(frame, *over, scene, &mut input);
            }
        }
        self.waited = 0;
        self.frame += 1;
        if self.frame >= step.frames() {
            self.step += 1;
            self.frame = 0;
        }
        Ok(input)
    }

    /// Hold this step for a target that is not there yet: patiently before
    /// `start`, not at all after it.
    fn wait(
        &mut self,
        in_setup: bool,
        input: FrameInput,
        what: &str,
    ) -> Result<FrameInput, String> {
        self.waited += 1;
        if in_setup && self.waited <= SETUP_PATIENCE {
            return Ok(input);
        }
        Err(format!("step {} never found {what}", self.step + 1))
    }

    fn screen_path(&self, target: &Target, scene: &dyn Scene) -> Result<Resolved, String> {
        let found = |lookup: Lookup, name: &str| match lookup {
            Lookup::Found(at) => Ok(Ok(at)),
            Lookup::Missing(near) if near.is_empty() => Ok(Err(name.to_string())),
            Lookup::Missing(near) => Ok(Err(format!("{name} (near: {})", near.join(", ")))),
            Lookup::Ambiguous(all) => Err(format!(
                "{name} matches {} controls ({}): name one",
                all.len(),
                all.join(", ")
            )),
        };
        let to = match target {
            Target::Px(at) => *at,
            Target::Widget(text) => match found(scene.widget(text), &format!("widget {text:?}"))? {
                Ok(at) => at,
                Err(what) => return Ok(Resolved::NotYet(what)),
            },
            Target::RightOf(text) => {
                match found(scene.right_of(text), &format!("right-of {text:?}"))? {
                    Ok(at) => at,
                    Err(what) => return Ok(Resolved::NotYet(what)),
                }
            }
            Target::Gizmo(axis) => {
                match scene
                    .gizmo_handle(*axis)
                    .and_then(|(handle, _)| scene.project(handle))
                {
                    Some(at) => at,
                    None => return Ok(Resolved::NotYet(format!("a gizmo {axis:?} handle"))),
                }
            }
            Target::Ground(at) => scene
                .ground(*at)
                .and_then(|world| scene.project(world))
                .ok_or_else(|| format!("ground {},{} is not on screen", at.x, at.y))?,
        };
        let from = self.pointer.unwrap_or(to);
        Ok(Resolved::Ready(Path::Screen { from, to }))
    }

    fn glide(&mut self, frame: u32, over: u32, scene: &dyn Scene, input: &mut FrameInput) {
        let t = smoothstep((frame + 1) as f32 / over as f32);
        let at = match self.path {
            Path::Still => None,
            Path::Screen { from, to } => Some(from.lerp(to, t)),
            Path::World { from, to, grab } => scene.project(from.lerp(to, t)).map(|at| at + grab),
        };
        if let Some(at) = at {
            input.moved = self.pointer != Some(at);
            self.pointer = Some(at);
            input.pointer = Some(at);
        }
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One AccessKit node from the last egui pass, in egui points.
#[derive(Clone, Debug)]
pub(crate) struct WidgetNode {
    role: accesskit::Role,
    label: Option<String>,
    value: Option<String>,
    rect: egui::Rect,
}

/// Every node the pass reported bounds for, minus text runs. egui hangs a
/// `TextRun` carrying the same text under each label, button and text field,
/// so leaving them in would make every plain label ambiguous with itself;
/// the widget above the run already has the text and the whole widget's
/// bounds.
pub(crate) fn widget_nodes(output: &egui::PlatformOutput) -> Vec<WidgetNode> {
    output
        .accesskit_update
        .iter()
        .flat_map(|update| &update.nodes)
        .filter(|(_, node)| node.role() != accesskit::Role::TextRun)
        .filter_map(|(_, node)| {
            let b = node.bounds()?;
            Some(WidgetNode {
                role: node.role(),
                label: node.label().map(str::to_string),
                value: node.value().map(str::to_string),
                rect: egui::Rect::from_min_max(
                    egui::pos2(b.x0 as f32, b.y0 as f32),
                    egui::pos2(b.x1 as f32, b.y1 as f32),
                ),
            })
        })
        .collect()
}

/// Whether a node is something a pointer operates, rather than text beside
/// one.
fn is_control(role: accesskit::Role) -> bool {
    use accesskit::Role;
    matches!(
        role,
        Role::Button
            | Role::CheckBox
            | Role::RadioButton
            | Role::ComboBox
            | Role::TextInput
            | Role::Link
            | Role::Slider
            | Role::SpinButton
            | Role::ColorWell
            | Role::TreeItem
            | Role::Tab
    )
}

fn centre_px(node: &WidgetNode, ppp: f32) -> Vec2 {
    let c = node.rect.center();
    Vec2::new(c.x * ppp, c.y * ppp)
}

fn describe(node: &WidgetNode) -> String {
    let text = node
        .label
        .as_deref()
        .or(node.value.as_deref())
        .unwrap_or("");
    format!("{:?} {text:?}", node.role)
}

/// Labels and values that contain `text`, for a lookup that found nothing.
fn near_misses(nodes: &[WidgetNode], text: &str) -> Vec<String> {
    let needle = text.to_lowercase();
    let mut near: Vec<String> = nodes
        .iter()
        .flat_map(|n| n.label.iter().chain(n.value.iter()))
        .filter(|s| !s.is_empty() && s.to_lowercase().contains(&needle))
        .cloned()
        .collect();
    near.sort();
    near.dedup();
    near.truncate(8);
    near
}

/// The one control named `text` by its label or value. When a control and a
/// label painted inside it both carry the text, the control is the one to
/// press.
pub(crate) fn find_widget(nodes: &[WidgetNode], text: &str, ppp: f32) -> Lookup {
    let named: Vec<&WidgetNode> = nodes
        .iter()
        .filter(|n| n.label.as_deref() == Some(text) || n.value.as_deref() == Some(text))
        .collect();
    let controls: Vec<&WidgetNode> = named
        .iter()
        .copied()
        .filter(|n| is_control(n.role))
        .collect();
    let pick = if controls.is_empty() { named } else { controls };
    match pick.as_slice() {
        [one] => Lookup::Found(centre_px(one, ppp)),
        [] => Lookup::Missing(near_misses(nodes, text)),
        many => Lookup::Ambiguous(many.iter().map(|n| describe(n)).collect()),
    }
}

/// The nearest control to the right of the label `text`, on its row. egui
/// reports a plain label's text as its AccessKit value, so the anchor is
/// matched on either.
pub(crate) fn find_right_of(nodes: &[WidgetNode], text: &str, ppp: f32) -> Lookup {
    let anchors: Vec<&WidgetNode> = nodes
        .iter()
        .filter(|n| n.label.as_deref() == Some(text) || n.value.as_deref() == Some(text))
        .collect();
    let anchor = match anchors.as_slice() {
        [one] => *one,
        [] => return Lookup::Missing(near_misses(nodes, text)),
        many => return Lookup::Ambiguous(many.iter().map(|n| describe(n)).collect()),
    };
    let row = anchor.rect.y_range();
    nodes
        .iter()
        .filter(|n| {
            is_control(n.role)
                && row.contains(n.rect.center().y)
                && n.rect.min.x >= anchor.rect.max.x - 1.0
        })
        .min_by(|a, b| a.rect.min.x.total_cmp(&b.rect.min.x))
        .map_or_else(
            || Lookup::Missing(Vec::new()),
            |n| Lookup::Found(centre_px(n, ppp)),
        )
}

/// The live scene: this frame's AccessKit tree, camera, gizmo and ground.
struct LiveScene<'a> {
    nodes: &'a [WidgetNode],
    ppp: f32,
    camera: Option<(&'a Camera, &'a GlobalTransform, f32)>,
    gizmo: Option<(Vec3, Quat)>,
    local: bool,
    heightmap: Option<&'a FinishedHeightMap>,
}

impl Scene for LiveScene<'_> {
    fn widget(&self, text: &str) -> Lookup {
        find_widget(self.nodes, text, self.ppp)
    }

    fn right_of(&self, text: &str) -> Lookup {
        find_right_of(self.nodes, text, self.ppp)
    }

    fn gizmo_handle(&self, axis: Axis) -> Option<(Vec3, Vec3)> {
        let (at, rotation) = self.gizmo?;
        let (camera, eye, fov) = self.camera?;
        let dir = if self.local {
            rotation * axis.unit()
        } else {
            axis.unit()
        };
        let per_px = world_per_pixel(camera, eye, fov, at)?;
        Some((at + dir * HANDLE_PX * per_px, dir))
    }

    fn ground(&self, at: Vec2) -> Option<Vec3> {
        self.heightmap
            .map(|h| Vec3::new(at.x, h.world_height_at(at.x, at.y), at.y))
    }

    fn project(&self, world: Vec3) -> Option<Vec2> {
        let (camera, eye, _) = self.camera?;
        camera.world_to_viewport(eye, world).ok()
    }
}

/// Metres per frame pixel at `at`'s depth: the scale the gizmo crate keeps
/// its handles a fixed size on screen by.
fn world_per_pixel(camera: &Camera, eye: &GlobalTransform, fov: f32, at: Vec3) -> Option<f32> {
    let depth = (at - eye.translation()).dot(*eye.forward());
    let height = camera.logical_viewport_size()?.y;
    (depth > 0.0).then(|| 2.0 * depth * (fov * 0.5).tan() / height)
}

/// Whether the script's setup steps have played - what the warm-up waits on.
#[derive(Resource, Debug)]
pub(crate) struct ScriptProgress {
    pub(crate) setup_done: bool,
}

/// The pointer as the last script frame left it, for the painted cursor.
#[derive(Resource, Debug, Default)]
pub(crate) struct Pointer {
    pos: Vec2,
    visible: bool,
    pressed: bool,
}

/// egui events a script frame produced, drained into the context's input
/// before its pass begins.
#[derive(Resource, Debug, Default)]
pub(crate) struct ScriptEguiEvents(pub(crate) Vec<egui::Event>);

/// Whether a script frame plays this frame. Only on a frame the clock
/// stepped, and never before the subject is framed: the clock also runs
/// while the world compiles, when the rig camera still sits on its spawn
/// placeholder, and a gizmo or ground target resolved then points at the
/// wrong place. The setup steps then play through the warm-up, and the
/// steps after `start` wait for the clip's first frame.
pub(crate) fn plays_this_frame(
    stepped: bool,
    framed: bool,
    setup_done: bool,
    clip_started: bool,
) -> bool {
    stepped && framed && (!setup_done || clip_started)
}

/// Play a script frame whenever [`plays_this_frame`] says so: every frame of
/// the warm-up for the setup steps, and once per captured frame after
/// `start`. Runs in `First`, after the clock, so bevy_picking, the input
/// systems and egui all read this frame's pointer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_script(
    clock: Res<Clock>,
    framed: Option<Res<super::super::headless::ClipTiming>>,
    started: Option<Res<ClipStarted>>,
    script: Res<EditorScript>,
    mut runner: ResMut<ScriptRunner>,
    mut progress: ResMut<ScriptProgress>,
    mut pointer: ResMut<Pointer>,
    mut egui_events: ResMut<ScriptEguiEvents>,
    mut contexts: Query<(&mut EguiContext, &EguiOutput), With<PrimaryEguiContext>>,
    cameras: Query<(&Camera, &GlobalTransform, &Projection), IsWorldCamera>,
    gizmo_targets: Query<&Transform, With<GizmoTarget>>,
    gizmo_options: Option<Res<GizmoOptions>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut pointer_inputs: MessageWriter<PointerInput>,
) {
    if !plays_this_frame(
        clock.stepped,
        framed.is_some(),
        runner.setup_done(&script),
        started.is_some(),
    ) {
        return;
    }
    let Ok((mut context, output)) = contexts.single_mut() else {
        return;
    };
    let ppp = context.get_mut().pixels_per_point();
    let nodes = widget_nodes(&output.platform_output);
    let camera = cameras
        .single()
        .ok()
        .and_then(|(camera, eye, projection)| match projection {
            Projection::Perspective(perspective) => Some((camera, eye, perspective.fov)),
            _ => None,
        });
    let scene = LiveScene {
        nodes: &nodes,
        ppp,
        camera,
        gizmo: gizmo_targets
            .iter()
            .next()
            .map(|t| (t.translation, t.rotation)),
        local: gizmo_options
            .as_ref()
            .is_some_and(|o| o.gizmo_orientation == GizmoOrientation::Local),
        heightmap: heightmap.as_deref(),
    };
    let (step_before, frame_before) = (runner.step, runner.frame);
    let input = runner
        .advance(&script, &scene)
        .unwrap_or_else(|e| panic!("--editor-script: {e}"));
    if frame_before == 0 && (runner.step, runner.frame) != (step_before, frame_before) {
        // One line per step as it starts, with where it put the pointer: the
        // trace to read when a gesture lands somewhere unexpected.
        info!(
            "--editor-script step {}: {:?}, pointer {:?}",
            step_before + 1,
            script.steps[step_before],
            input.pointer
        );
    }
    let done = runner.setup_done(&script);
    if progress.setup_done != done {
        progress.setup_done = done;
    }

    let Some(at) = input.pointer else {
        return;
    };
    let Ok((window_entity, mut window)) = windows.single_mut() else {
        return;
    };
    let egui_at = egui::pos2(at.x / ppp, at.y / ppp);
    if input.moved || !pointer.visible {
        let delta = if pointer.visible {
            at - pointer.pos
        } else {
            Vec2::ZERO
        };
        pointer.pos = at;
        pointer.visible = true;
        window.set_physical_cursor_position(Some(at.as_dvec2()));
        if let Some(target) =
            RenderTarget::Window(WindowRef::Entity(window_entity)).normalize(Some(window_entity))
        {
            pointer_inputs.write(PointerInput::new(
                PointerId::Mouse,
                Location {
                    target,
                    position: at,
                },
                PointerAction::Move { delta },
            ));
        }
        egui_events.0.push(egui::Event::PointerMoved(egui_at));
    }
    for (edge, state, pressed) in [
        (input.press, ButtonState::Pressed, true),
        (input.release, ButtonState::Released, false),
    ] {
        if !edge {
            continue;
        }
        pointer.pressed = pressed;
        buttons.write(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window: window_entity,
        });
        egui_events.0.push(egui::Event::PointerButton {
            pos: egui_at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    if let Some(text) = input.typed {
        // Select everything in the focused field first, so the text replaces
        // what was there, as a person retyping a seed would.
        for pressed in [true, false] {
            egui_events.0.push(egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            });
        }
        egui_events.0.push(egui::Event::Text(text));
    }
    if let Some(by) = input.scrolled {
        // egui's delta is the CONTENT's movement, so scrolling a list down
        // moves its content up: the sign flips here rather than in the
        // script, where "scroll 200" should mean "200 points further down
        // the list".
        egui_events.0.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::Vec2::new(0.0, -by),
            // What egui asks an integration to send when the phase is not a
            // trackpad's to know.
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
    }
}

/// Draw the pointer: an arrow above every window, and a soft ring while the
/// button is down, so a still or a clip shows what is pointed at and when it
/// is pressed.
pub(crate) fn paint_cursor(
    pointer: Option<Res<Pointer>>,
    mut contexts: EguiContexts,
    mut announced: Local<bool>,
) {
    let Some(pointer) = pointer.filter(|p| p.visible) else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !std::mem::replace(&mut *announced, true) {
        info!(
            "--editor-script: painting the cursor from {} (content rect {:?})",
            pointer.pos,
            ctx.content_rect()
        );
    }
    let ppp = ctx.pixels_per_point();
    let tip = egui::pos2(pointer.pos.x / ppp, pointer.pos.y / ppp);
    let scale = CURSOR_SCALE / ppp;
    let at = |x: f32, y: f32| tip + egui::vec2(x * scale, y * scale);
    // Unclipped: the cursor belongs to the frame, not to any panel's rect.
    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Debug,
            egui::Id::new("render_tool_cursor"),
        ))
        .with_clip_rect(egui::Rect::EVERYTHING);
    if pointer.pressed {
        painter.circle_filled(
            tip,
            10.0 * scale,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 110),
        );
    }
    // The classic arrow: filled as three convex pieces, outlined once.
    let white = egui::Color32::WHITE;
    for piece in [
        vec![at(0.0, 0.0), at(4.0, 13.0), at(0.0, 17.0)],
        vec![at(0.0, 0.0), at(12.0, 12.0), at(4.0, 13.0)],
        vec![at(4.0, 13.0), at(7.0, 12.0), at(10.0, 19.0), at(7.0, 20.0)],
    ] {
        painter.add(egui::Shape::convex_polygon(
            piece,
            white,
            egui::Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::closed_line(
        vec![
            at(0.0, 0.0),
            at(0.0, 17.0),
            at(4.0, 13.0),
            at(7.0, 20.0),
            at(10.0, 19.0),
            at(7.0, 12.0),
            at(12.0, 12.0),
        ],
        egui::Stroke::new(1.2, egui::Color32::BLACK),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A top-down scene: 10 px per metre, widgets at fixed points.
    struct FakeScene {
        widgets: Vec<(&'static str, Vec2)>,
        handle: Option<(Vec3, Vec3)>,
    }

    impl Scene for FakeScene {
        fn widget(&self, text: &str) -> Lookup {
            let hits: Vec<&(&str, Vec2)> =
                self.widgets.iter().filter(|(t, _)| *t == text).collect();
            match hits.as_slice() {
                [(_, at)] => Lookup::Found(*at),
                [] => Lookup::Missing(Vec::new()),
                many => Lookup::Ambiguous(many.iter().map(|(t, _)| t.to_string()).collect()),
            }
        }

        fn right_of(&self, _text: &str) -> Lookup {
            Lookup::Missing(Vec::new())
        }

        fn gizmo_handle(&self, _axis: Axis) -> Option<(Vec3, Vec3)> {
            self.handle
        }

        fn ground(&self, at: Vec2) -> Option<Vec3> {
            Some(Vec3::new(at.x, 0.0, at.y))
        }

        fn project(&self, world: Vec3) -> Option<Vec2> {
            Some(Vec2::new(world.x * 10.0, world.z * 10.0))
        }
    }

    fn play(source: &str, scene: &FakeScene, frames: usize) -> Vec<FrameInput> {
        let script = EditorScript::parse(source).expect("the script parses");
        let mut runner = ScriptRunner::default();
        (0..frames)
            .map(|_| runner.advance(&script, scene).expect("the frame plays"))
            .collect()
    }

    #[test]
    fn every_step_and_target_parses() {
        let script = EditorScript::parse(
            "# a comment\n\
             click widget \"#9 Placements\" over 8  # trailing\n\
             hold 3\n\
             start\n\
             move right-of \"Theme:\"\n\
             move px 640,360 over 2\n\
             move gizmo z\n\
             press\n\
             drag-gizmo x -6.5 over 20\n\
             release\n\
             move ground 118,-3 over 20\n\
             type \"253\"\n",
        )
        .expect("the script parses");
        assert_eq!(script.start, 2);
        assert_eq!(
            script.steps,
            vec![
                Step::Click {
                    on: Target::Widget("#9 Placements".into()),
                    over: 8
                },
                Step::Hold(3),
                Step::Move {
                    to: Target::RightOf("Theme:".into()),
                    over: 1
                },
                Step::Move {
                    to: Target::Px(Vec2::new(640.0, 360.0)),
                    over: 2
                },
                Step::Move {
                    to: Target::Gizmo(Axis::Z),
                    over: 1
                },
                Step::Press,
                Step::DragGizmo {
                    axis: Axis::X,
                    metres: -6.5,
                    over: 20
                },
                Step::Release,
                Step::Move {
                    to: Target::Ground(Vec2::new(118.0, -3.0)),
                    over: 20
                },
                Step::Type("253".into()),
            ]
        );
        assert_eq!(script.steps[0].frames(), 8 + CLICK_TAIL);
    }

    #[test]
    fn a_bad_line_is_refused_by_its_number() {
        for (source, line, words) in [
            ("hold 2\nwiggle 3", 2, "unknown step"),
            ("click widget Undo", 1, "quoted"),
            ("hold 0", 1, "at least 1"),
            ("start\nhold 1\nstart", 3, "twice"),
            ("drag-gizmo w 2 over 3", 1, "axis"),
            ("type \"never closed", 1, "never closed"),
            ("press now", 1, "takes nothing"),
        ] {
            let err = EditorScript::parse(source).unwrap_err();
            assert!(
                err.starts_with(&format!("line {line}:")) && err.contains(words),
                "{source:?}: {err}"
            );
        }
    }

    #[test]
    fn a_click_glides_rests_presses_and_releases_on_the_frames_it_names() {
        let scene = FakeScene {
            widgets: vec![("Undo", Vec2::new(200.0, 40.0))],
            handle: None,
        };
        let frames = play(
            "move px 100,100\nclick widget \"Undo\" over 4\ntype \"253\"",
            &scene,
            9,
        );
        assert_eq!(frames[0].pointer, Some(Vec2::new(100.0, 100.0)));
        assert!(
            frames[1..=4]
                .iter()
                .all(|f| f.moved && !f.press && !f.release)
        );
        assert_eq!(
            frames[4].pointer,
            Some(Vec2::new(200.0, 40.0)),
            "lands exactly"
        );
        assert!(
            !frames[5].moved && !frames[5].press,
            "rests a frame on the target"
        );
        assert!(frames[6].press && !frames[6].release);
        assert!(frames[7].release && !frames[7].press);
        assert_eq!(frames[8].typed.as_deref(), Some("253"));
        assert_eq!(frames[8].pointer, Some(Vec2::new(200.0, 40.0)));
    }

    #[test]
    fn a_gizmo_drag_pulls_the_pointer_along_the_axis_in_world_space() {
        let scene = FakeScene {
            widgets: Vec::new(),
            handle: Some((Vec3::new(1.0, 0.0, 2.0), Vec3::X)),
        };
        let frames = play(
            "move gizmo x\npress\ndrag-gizmo x 3.0 over 5\nrelease",
            &scene,
            8,
        );
        assert_eq!(frames[0].pointer, Some(Vec2::new(10.0, 20.0)));
        assert!(frames[1].press);
        // Five frames on, the handle's projection is 3 m along +X: 30 px.
        assert_eq!(frames[6].pointer, Some(Vec2::new(40.0, 20.0)));
        assert!(frames[2..=6].iter().all(|f| f.moved));
        assert!(frames[7].release);
    }

    #[test]
    fn a_setup_step_waits_for_its_widget_but_a_shot_step_does_not() {
        let scene = FakeScene {
            widgets: Vec::new(),
            handle: None,
        };
        let setup = EditorScript::parse("click widget \"Catalogue\"\nstart").unwrap();
        let mut runner = ScriptRunner::default();
        for _ in 0..SETUP_PATIENCE {
            runner.advance(&setup, &scene).expect("a setup step waits");
        }
        let err = runner.advance(&setup, &scene).unwrap_err();
        assert!(err.contains("Catalogue"), "{err}");
        let shot = EditorScript::parse("click widget \"Catalogue\"").unwrap();
        assert!(ScriptRunner::default().advance(&shot, &scene).is_err());
    }

    fn node(role: accesskit::Role, label: &str, r: [f32; 4]) -> WidgetNode {
        WidgetNode {
            role,
            label: Some(label.to_string()),
            value: None,
            rect: egui::Rect::from_min_max(egui::pos2(r[0], r[1]), egui::pos2(r[2], r[3])),
        }
    }

    #[test]
    fn right_of_takes_the_nearest_control_on_the_label_row() {
        use accesskit::Role;
        let nodes = vec![
            node(Role::Label, "Biome:", [10.0, 10.0, 60.0, 30.0]),
            node(Role::Button, "🔓", [70.0, 10.0, 90.0, 30.0]),
            node(Role::Label, "Theme:", [10.0, 40.0, 60.0, 60.0]),
            node(Role::ComboBox, "", [100.0, 40.0, 200.0, 60.0]),
            node(Role::Button, "🔓", [70.0, 40.0, 90.0, 60.0]),
        ];
        let Lookup::Found(at) = find_right_of(&nodes, "Theme:", 2.0) else {
            panic!("the Theme padlock is found");
        };
        assert_eq!(
            at,
            Vec2::new(160.0, 100.0),
            "centre (80, 50) in points, doubled"
        );
        assert!(matches!(
            find_widget(&nodes, "🔓", 1.0),
            Lookup::Ambiguous(ref all) if all.len() == 2
        ));
    }

    /// The addressing end to end on real egui: the Undo button is found in the
    /// pass's AccessKit tree by its label, and a press and release at that
    /// point click it.
    #[test]
    fn a_widget_found_by_its_label_takes_a_real_click() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 360.0));
        let mut clicks = 0;
        let mut pending: Vec<egui::Event> = Vec::new();
        let mut aim = None;
        for frame in 0..8u32 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                time: Some(f64::from(frame) * 0.08),
                events: std::mem::take(&mut pending),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                egui::Window::new("World Editor").show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Theme:");
                        if ui.button("Undo").clicked() {
                            clicks += 1;
                        }
                    });
                });
            });
            let nodes = widget_nodes(&output.platform_output);
            match frame {
                // A new window's first frame is an invisible sizing pass.
                2 => {
                    let Lookup::Found(at) = find_widget(&nodes, "Undo", 1.0) else {
                        panic!("the Undo button is in the AccessKit tree");
                    };
                    // egui reports a plain label's text as its value, not its
                    // label: `right-of` has to find the button beside it anyway.
                    let Lookup::Found(beside) = find_right_of(&nodes, "Theme:", 1.0) else {
                        panic!("the control right of the Theme: label is found");
                    };
                    assert_eq!(beside, at, "right-of lands on the same button");
                    let pos = egui::pos2(at.x, at.y);
                    aim = Some(pos);
                    pending = vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ];
                }
                3 => {
                    pending = vec![egui::Event::PointerButton {
                        pos: aim.expect("aimed on frame 2"),
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }];
                }
                _ => {}
            }
        }
        assert_eq!(clicks, 1);
    }
}

#[cfg(test)]
mod gating_tests {
    use super::plays_this_frame;

    #[test]
    fn a_script_plays_only_on_a_stepped_frame_of_a_framed_shot() {
        // The world is still compiling: the camera is not aimed yet.
        assert!(!plays_this_frame(true, false, false, false));
        // Framed and warming up: the setup steps play on each step.
        assert!(plays_this_frame(true, true, false, false));
        assert!(!plays_this_frame(false, true, false, false));
        // Setup done, clip not started: the shot steps wait for frame 0.
        assert!(!plays_this_frame(true, true, true, false));
        // The clip has started: one step per captured frame.
        assert!(plays_this_frame(true, true, true, true));
    }
}
