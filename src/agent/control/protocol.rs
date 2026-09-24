//! What travels over the control socket (#1416): one [`Request`] line in,
//! one [`Response`] line out.

use serde::{Deserialize, Serialize};

/// A command, as a client sends it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Request {
    /// Who, where and with whom the agent is.
    Status,
    /// What has happened after event `since`, waiting up to `wait_secs` for
    /// something to.
    Events { since: u64, wait_secs: u64 },
    /// Leave the world and end the daemon. The saved session is kept.
    Stop,
    /// Say `text` in the room, as the chat window would.
    Say { text: String },
    /// Walk (or drive) to the point `x`, `z` on the ground, running if `run`.
    WalkTo {
        x: f32,
        z: f32,
        #[serde(default)]
        run: bool,
    },
    /// Stop walking.
    Halt,
    /// Travel to the world of `room_did`, which the operator called `label`,
    /// doing `unsaved` with any unsaved edits to the agent's own world.
    Travel {
        room_did: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        unsaved: UnsavedEdits,
    },
    /// Take a picture of what the agent sees and write it to a PNG.
    Look(LookSpec),
    /// Follow the player `did`, `distance` metres behind (a default when
    /// absent), running if `run`.
    Follow {
        did: String,
        #[serde(default)]
        distance: Option<f32>,
        #[serde(default)]
        run: bool,
    },
    /// Turn to face the player `did`, or the point `at` (x, z) - one of them.
    Face {
        #[serde(default)]
        did: Option<String>,
        #[serde(default)]
        at: Option<[f32; 2]>,
    },
    /// The things placed in the agent's world, by index, each where it is
    /// drawn - within `within_m` of the agent when given (#1422).
    Placements {
        #[serde(default)]
        within_m: Option<f32>,
    },
    /// The catalogue's entries - those matching `search`, when given.
    Catalogue {
        #[serde(default)]
        search: Option<String>,
    },
    /// Put the catalogue entry `slug` down on the ground at `at` (x, z) - a
    /// few metres ahead of the agent when absent - turned to `yaw_deg`.
    Place {
        slug: String,
        #[serde(default)]
        at: Option<[f32; 2]>,
        #[serde(default)]
        yaw_deg: Option<f32>,
    },
    /// Move the placement `index` to (`x`, `z`), keeping its height above
    /// the ground, and turn it to `yaw_deg` when given.
    Move {
        index: usize,
        x: f32,
        z: f32,
        #[serde(default)]
        yaw_deg: Option<f32>,
    },
    /// Take the placement `index` out of the world.
    Remove { index: usize },
    /// The world's record as the JSON the World Editor's Raw JSON tab
    /// shows, or the part of it at `pointer` (RFC 6901; empty for all).
    RoomGet {
        #[serde(default)]
        pointer: String,
    },
    /// Replace the part of the world's record at `pointer` with `value`.
    RoomSet {
        pointer: String,
        value: serde_json::Value,
    },
    /// The agent's avatar as JSON - its record, and the body and worn items
    /// a rigged body keeps beside it - or the part at `pointer`.
    AvatarGet {
        #[serde(default)]
        pointer: String,
    },
    /// Replace the part of the avatar's JSON at `pointer` with `value`.
    AvatarSet {
        pointer: String,
        value: serde_json::Value,
    },
    /// Step back one edit of `record`, as Ctrl+Z does.
    Undo {
        #[serde(default)]
        record: EditRecord,
    },
    /// Step forward again one edit of `record` that was undone.
    Redo {
        #[serde(default)]
        record: EditRecord,
    },
    /// Throw `record`'s unsaved edits away: back to what was last saved.
    Revert {
        #[serde(default)]
        record: EditRecord,
    },
    /// Save `record` to the agent's account - only when the daemon was
    /// started with `--allow-save`.
    Save {
        #[serde(default)]
        record: EditRecord,
    },
    /// What the agent's inventory holds (#1423).
    Inventory,
    /// Put `what` into the inventory: a thing in the agent's own world, by
    /// its name, or a catalogue entry, by its slug.
    Stash { what: String },
    /// Take the item `name` out of the inventory.
    Unstash { name: String },
    /// Put the inventory item `name` on the avatar.
    Wear { name: String },
    /// Take the worn item `name` off the avatar.
    TakeOff { name: String },
    /// Offer `item` - an inventory item by its name, or a catalogue entry by
    /// its slug - to the player `to_did`, who is in the agent's world.
    GiftGive { to_did: String, item: String },
    /// Accept the gift the agent was offered as `offer_id`.
    GiftAccept { offer_id: u64 },
    /// Decline it.
    GiftDecline { offer_id: u64 },
    /// The interface (#1424): what is open, and what can be - with a
    /// picture of it when `picture`.
    Ui {
        #[serde(default)]
        picture: bool,
    },
    /// One window's controls and words - or the dialog the agent's click
    /// raised - each by the path the interface commands take, with a
    /// picture of the window when `picture`.
    UiShow {
        window: String,
        #[serde(default)]
        picture: bool,
    },
    /// Open a window, as its toolbar button does, and list it.
    UiOpen { window: String },
    /// Close a window.
    UiClose { window: String },
    /// Click the control at `path`.
    UiClick { path: String },
    /// Type `text` into the field at `path`, replacing what it holds, and
    /// press Enter after when `enter`.
    UiType {
        path: String,
        text: String,
        #[serde(default)]
        enter: bool,
    },
    /// Set the slider or number at `path` to `value`.
    UiSet { path: String, value: f64 },
    /// Pick `option` from the combo box or menu at `path`.
    UiChoose { path: String, option: String },
    /// Scroll `window`'s list by `points`: down when positive.
    UiScroll { window: String, points: f32 },
}

/// An interface command (#1424), as the world answers it.
#[derive(Debug, Clone, PartialEq)]
pub enum UiRequest {
    Summary {
        picture: bool,
    },
    Show {
        window: String,
        picture: bool,
    },
    Open {
        window: String,
    },
    Close {
        window: String,
    },
    Click {
        path: String,
    },
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

impl UiRequest {
    /// Whether it works a control, which may write one of the agent's
    /// records - the way an edit may.
    pub fn acts(&self) -> bool {
        matches!(
            self,
            Self::Click { .. } | Self::Type { .. } | Self::Set { .. } | Self::Choose { .. }
        )
    }
}

/// Which of the agent's records an edit command is about (#1422).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum EditRecord {
    /// The agent's own world.
    #[default]
    Room,
    /// The agent's avatar.
    Avatar,
    /// The agent's inventory (#1423): saved and reverted, but with no undo
    /// history - the game keeps none for it.
    Inventory,
}

impl EditRecord {
    /// The record's name in answers and events.
    pub fn word(self) -> &'static str {
        match self {
            Self::Room => "room",
            Self::Avatar => "avatar",
            Self::Inventory => "inventory",
        }
    }
}

/// What a trip does with unsaved edits to the agent's own world, which
/// leaving it would lose (#1422).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnsavedEdits {
    /// Do not leave: the trip is refused, and says why.
    #[default]
    Refuse,
    /// Leave and lose them - the travel dialog's "Discard & travel".
    Discard,
    /// Save them, and leave once the save has landed - "Save & travel".
    Save,
}

/// Which picture `look` takes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct LookSpec {
    #[serde(default)]
    pub view: LookView,
    /// Which way to look, in degrees clockwise from where the agent faces:
    /// 0 ahead, 90 right, 180 behind, -90 left.
    #[serde(default)]
    pub heading_deg: Option<f32>,
    /// A point on the ground to look toward, as (x, z), instead.
    #[serde(default)]
    pub at: Option<[f32; 2]>,
    /// Where to write the picture; the agent's own directory by default.
    #[serde(default)]
    pub out: Option<std::path::PathBuf>,
}

/// Where a picture is taken from.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum LookView {
    /// The game's own camera, behind and above the body - what a person
    /// playing sees.
    #[default]
    Play,
    /// From the body's eyes: the front of it, near its top, looking level.
    Eyes,
}

/// Where a [`Request`] is answered.
pub enum Route {
    /// By the control socket itself, from the event log.
    Events { since: u64, wait_secs: u64 },
    /// By the daemon, in the world, on its next frame.
    World(WorldRequest),
}

/// A request the daemon answers from the world.
#[derive(Debug, Clone, PartialEq)]
pub enum WorldRequest {
    Status,
    Stop,
    Say(String),
    WalkTo {
        x: f32,
        z: f32,
        run: bool,
    },
    Halt,
    Travel {
        room_did: String,
        label: Option<String>,
        unsaved: UnsavedEdits,
    },
    Look(LookSpec),
    Follow {
        did: String,
        distance: f32,
        run: bool,
    },
    Face {
        did: Option<String>,
        at: Option<[f32; 2]>,
    },
    /// An edit command (#1422).
    Edit(EditRequest),
    /// A gift command (#1423).
    Gift(GiftRequest),
    /// An interface command (#1424).
    Ui(UiRequest),
}

/// A request that offers a gift, or answers one (#1423).
#[derive(Debug, Clone, PartialEq)]
pub enum GiftRequest {
    Give { to_did: String, item: String },
    Accept(u64),
    Decline(u64),
}

/// A request that reads or edits the agent's world or avatar (#1422).
#[derive(Debug, Clone, PartialEq)]
pub enum EditRequest {
    Placements {
        within_m: Option<f32>,
    },
    Catalogue {
        search: Option<String>,
    },
    Place {
        slug: String,
        at: Option<[f32; 2]>,
        yaw_deg: Option<f32>,
    },
    Move {
        index: usize,
        x: f32,
        z: f32,
        yaw_deg: Option<f32>,
    },
    Remove {
        index: usize,
    },
    Get {
        record: EditRecord,
        pointer: String,
    },
    Set {
        record: EditRecord,
        pointer: String,
        value: serde_json::Value,
    },
    Undo(EditRecord),
    Redo(EditRecord),
    Revert(EditRecord),
    Save(EditRecord),
    Inventory,
    Stash {
        what: String,
    },
    Unstash {
        name: String,
    },
    Wear {
        name: String,
    },
    TakeOff {
        name: String,
    },
}

impl EditRequest {
    /// Whether answering this may write one of the agent's records. The
    /// daemon answers at most one of these a frame: the undo history takes
    /// one entry per frame a record changed in, so two edits answered in
    /// one frame would be one step to undo.
    pub fn writes(&self) -> bool {
        !matches!(
            self,
            Self::Placements { .. }
                | Self::Catalogue { .. }
                | Self::Get { .. }
                | Self::Save(_)
                | Self::Inventory
        )
    }
}

impl WorldRequest {
    /// Whether answering this may write one of the agent's records - an
    /// edit, or a trip that throws the world's unsaved edits away first.
    /// See [`EditRequest::writes`].
    pub fn writes_a_record(&self) -> bool {
        match self {
            Self::Edit(edit) => edit.writes(),
            // Accepting puts the gift in the inventory.
            Self::Gift(GiftRequest::Accept(_)) => true,
            // A control worked in an editor writes its record, as an edit.
            Self::Ui(ui) => ui.acts(),
            Self::Travel {
                unsaved: UnsavedEdits::Discard,
                ..
            } => true,
            _ => false,
        }
    }

    /// How long the world may take to answer: a frame for most requests, a
    /// render for a picture.
    pub fn answer_within(&self) -> std::time::Duration {
        use crate::config::agent::{LOOK_ANSWER_TIMEOUT, UI_ANSWER_TIMEOUT, WORLD_ANSWER_TIMEOUT};
        match self {
            Self::Look(_) => LOOK_ANSWER_TIMEOUT,
            Self::Ui(_) => UI_ANSWER_TIMEOUT,
            _ => WORLD_ANSWER_TIMEOUT,
        }
    }
}

impl Request {
    pub fn route(self) -> Route {
        match self {
            Self::Events { since, wait_secs } => Route::Events { since, wait_secs },
            Self::Status => Route::World(WorldRequest::Status),
            Self::Stop => Route::World(WorldRequest::Stop),
            Self::Say { text } => Route::World(WorldRequest::Say(text)),
            Self::WalkTo { x, z, run } => Route::World(WorldRequest::WalkTo { x, z, run }),
            Self::Halt => Route::World(WorldRequest::Halt),
            Self::Travel {
                room_did,
                label,
                unsaved,
            } => Route::World(WorldRequest::Travel {
                room_did,
                label,
                unsaved,
            }),
            Self::Look(spec) => Route::World(WorldRequest::Look(spec)),
            Self::Follow { did, distance, run } => Route::World(WorldRequest::Follow {
                did,
                distance: distance.unwrap_or(crate::config::agent::FOLLOW_DISTANCE_M),
                run,
            }),
            Self::Face { did, at } => Route::World(WorldRequest::Face { did, at }),
            Self::Placements { within_m } => edit(EditRequest::Placements { within_m }),
            Self::Catalogue { search } => edit(EditRequest::Catalogue { search }),
            Self::Place { slug, at, yaw_deg } => edit(EditRequest::Place { slug, at, yaw_deg }),
            Self::Move {
                index,
                x,
                z,
                yaw_deg,
            } => edit(EditRequest::Move {
                index,
                x,
                z,
                yaw_deg,
            }),
            Self::Remove { index } => edit(EditRequest::Remove { index }),
            Self::RoomGet { pointer } => edit(EditRequest::Get {
                record: EditRecord::Room,
                pointer,
            }),
            Self::RoomSet { pointer, value } => edit(EditRequest::Set {
                record: EditRecord::Room,
                pointer,
                value,
            }),
            Self::AvatarGet { pointer } => edit(EditRequest::Get {
                record: EditRecord::Avatar,
                pointer,
            }),
            Self::AvatarSet { pointer, value } => edit(EditRequest::Set {
                record: EditRecord::Avatar,
                pointer,
                value,
            }),
            Self::Undo { record } => edit(EditRequest::Undo(record)),
            Self::Redo { record } => edit(EditRequest::Redo(record)),
            Self::Revert { record } => edit(EditRequest::Revert(record)),
            Self::Save { record } => edit(EditRequest::Save(record)),
            Self::Inventory => edit(EditRequest::Inventory),
            Self::Stash { what } => edit(EditRequest::Stash { what }),
            Self::Unstash { name } => edit(EditRequest::Unstash { name }),
            Self::Wear { name } => edit(EditRequest::Wear { name }),
            Self::TakeOff { name } => edit(EditRequest::TakeOff { name }),
            Self::GiftGive { to_did, item } => {
                Route::World(WorldRequest::Gift(GiftRequest::Give { to_did, item }))
            }
            Self::GiftAccept { offer_id } => {
                Route::World(WorldRequest::Gift(GiftRequest::Accept(offer_id)))
            }
            Self::GiftDecline { offer_id } => {
                Route::World(WorldRequest::Gift(GiftRequest::Decline(offer_id)))
            }
            Self::Ui { picture } => ui(UiRequest::Summary { picture }),
            Self::UiShow { window, picture } => ui(UiRequest::Show { window, picture }),
            Self::UiOpen { window } => ui(UiRequest::Open { window }),
            Self::UiClose { window } => ui(UiRequest::Close { window }),
            Self::UiClick { path } => ui(UiRequest::Click { path }),
            Self::UiType { path, text, enter } => ui(UiRequest::Type { path, text, enter }),
            Self::UiSet { path, value } => ui(UiRequest::Set { path, value }),
            Self::UiChoose { path, option } => ui(UiRequest::Choose { path, option }),
            Self::UiScroll { window, points } => ui(UiRequest::Scroll { window, points }),
        }
    }
}

fn ui(request: UiRequest) -> Route {
    Route::World(WorldRequest::Ui(request))
}

fn edit(request: EditRequest) -> Route {
    Route::World(WorldRequest::Edit(request))
}

/// The answer to one request: `{"ok":true,"result":…}` or
/// `{"ok":false,"error":"…"}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn success(result: serde_json::Value) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire form is what an agent - or a person with `socat` - writes, so
    /// it is pinned here rather than left to whatever serde's defaults are.
    #[test]
    fn requests_are_one_tagged_object_each() {
        assert_eq!(
            serde_json::to_string(&Request::Status).unwrap(),
            r#"{"command":"status"}"#
        );
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"events","since":4,"wait_secs":30}"#)
                .unwrap(),
            Request::Events {
                since: 4,
                wait_secs: 30
            }
        );
        assert!(serde_json::from_str::<Request>(r#"{"command":"teleport"}"#).is_err());
    }

    /// A look's spec sits beside its command, and everything in it is
    /// optional: a bare `look` is the game's view straight ahead.
    #[test]
    fn a_look_is_one_flat_object_with_every_field_optional() {
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"look"}"#).unwrap(),
            Request::Look(LookSpec::default())
        );
        let eyes = Request::Look(LookSpec {
            view: LookView::Eyes,
            heading_deg: Some(-90.0),
            at: None,
            out: None,
        });
        let wire = serde_json::to_value(&eyes).unwrap();
        assert_eq!(wire["command"], "look");
        assert_eq!(wire["view"], "eyes");
        assert_eq!(serde_json::from_value::<Request>(wire).unwrap(), eyes);
        assert!(
            serde_json::from_str::<Request>(r#"{"command":"look","view":"sideways"}"#).is_err()
        );
    }

    /// A picture may take a render to answer; nothing else may take longer
    /// than a frame's grace.
    #[test]
    fn only_a_look_waits_longer_than_the_world_answers() {
        use crate::config::agent::{LOOK_ANSWER_TIMEOUT, WORLD_ANSWER_TIMEOUT};
        assert_eq!(
            WorldRequest::Look(LookSpec::default()).answer_within(),
            LOOK_ANSWER_TIMEOUT
        );
        assert_eq!(WorldRequest::Status.answer_within(), WORLD_ANSWER_TIMEOUT);
    }

    /// The edit commands' wire forms (#1422): a record defaults to the
    /// world, and a trip to refusing over unsaved edits.
    #[test]
    fn edit_requests_are_flat_objects_with_their_defaults() {
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"undo"}"#).unwrap(),
            Request::Undo {
                record: EditRecord::Room
            }
        );
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"save","record":"avatar"}"#).unwrap(),
            Request::Save {
                record: EditRecord::Avatar
            }
        );
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"room_get"}"#).unwrap(),
            Request::RoomGet {
                pointer: String::new()
            }
        );
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"place","slug":"lamp","at":[1.5,-2]}"#)
                .unwrap(),
            Request::Place {
                slug: "lamp".into(),
                at: Some([1.5, -2.0]),
                yaw_deg: None
            }
        );
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"travel","room_did":"did:plc:a"}"#)
                .unwrap(),
            Request::Travel {
                room_did: "did:plc:a".into(),
                label: None,
                unsaved: UnsavedEdits::Refuse
            }
        );
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"save","record":"inventory"}"#).unwrap(),
            Request::Save {
                record: EditRecord::Inventory
            }
        );
        assert!(serde_json::from_str::<Request>(r#"{"command":"save","record":"world"}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"gift_accept","offer_id":7}"#).unwrap(),
            Request::GiftAccept { offer_id: 7 }
        );
    }

    /// What may write a record is what ends the daemon's turn for a frame
    /// (`serve`); reads and saves do not write one.
    #[test]
    fn only_what_writes_a_record_ends_a_frames_turn() {
        let writes = |request: Request| match request.route() {
            Route::World(world) => world.writes_a_record(),
            Route::Events { .. } => false,
        };
        for writer in [
            Request::Place {
                slug: "lamp".into(),
                at: None,
                yaw_deg: None,
            },
            Request::Remove { index: 0 },
            Request::RoomSet {
                pointer: String::new(),
                value: serde_json::json!({}),
            },
            Request::AvatarSet {
                pointer: String::new(),
                value: serde_json::json!({}),
            },
            Request::Undo {
                record: EditRecord::Avatar,
            },
            Request::Revert {
                record: EditRecord::Room,
            },
            Request::Travel {
                room_did: "did:plc:a".into(),
                label: None,
                unsaved: UnsavedEdits::Discard,
            },
            Request::Stash {
                what: "lamp".into(),
            },
            Request::Unstash {
                name: "lamp".into(),
            },
            Request::Wear { name: "hat".into() },
            Request::TakeOff { name: "hat".into() },
            Request::GiftAccept { offer_id: 1 },
        ] {
            assert!(writes(writer.clone()), "{writer:?}");
        }
        for reader in [
            Request::Status,
            Request::Placements { within_m: None },
            Request::RoomGet {
                pointer: String::new(),
            },
            Request::Save {
                record: EditRecord::Room,
            },
            Request::Travel {
                room_did: "did:plc:a".into(),
                label: None,
                unsaved: UnsavedEdits::Save,
            },
            Request::Inventory,
            Request::GiftDecline { offer_id: 1 },
            Request::GiftGive {
                to_did: "did:plc:a".into(),
                item: "lamp".into(),
            },
        ] {
            assert!(!writes(reader.clone()), "{reader:?}");
        }
    }

    /// The interface commands' wire forms (#1424): one flat object each,
    /// Enter off unless asked. Only the ones that work a control may write
    /// a record, so only they end a frame's turn - and every one may wait
    /// as long as the interface takes.
    #[test]
    fn interface_requests_are_flat_and_only_working_a_control_writes() {
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"command":"ui"}"#).unwrap(),
            Request::Ui { picture: false }
        );
        assert_eq!(
            serde_json::from_str::<Request>(
                r#"{"command":"ui_type","path":"Avatar > Search","text":"lamp"}"#
            )
            .unwrap(),
            Request::UiType {
                path: "Avatar > Search".into(),
                text: "lamp".into(),
                enter: false
            }
        );
        let writes = |request: Request| match request.route() {
            Route::World(world) => world.writes_a_record(),
            Route::Events { .. } => false,
        };
        let path = || "Avatar > Re-roll".to_owned();
        for acting in [
            Request::UiClick { path: path() },
            Request::UiType {
                path: path(),
                text: "x".into(),
                enter: true,
            },
            Request::UiSet {
                path: path(),
                value: 1.0,
            },
            Request::UiChoose {
                path: path(),
                option: "Light".into(),
            },
        ] {
            assert!(writes(acting.clone()), "{acting:?}");
        }
        for reading in [
            Request::Ui { picture: true },
            Request::UiShow {
                window: "Avatar".into(),
                picture: false,
            },
            Request::UiOpen {
                window: "Avatar".into(),
            },
            Request::UiClose {
                window: "Avatar".into(),
            },
            Request::UiScroll {
                window: "Avatar".into(),
                points: 200.0,
            },
        ] {
            assert!(!writes(reading.clone()), "{reading:?}");
        }
        assert_eq!(
            WorldRequest::Ui(UiRequest::Summary { picture: false }).answer_within(),
            crate::config::agent::UI_ANSWER_TIMEOUT
        );
    }

    #[test]
    fn a_response_carries_a_result_or_an_error_never_both() {
        let ok = serde_json::to_value(Response::success(serde_json::json!({"x": 1}))).unwrap();
        assert_eq!(ok, serde_json::json!({"ok": true, "result": {"x": 1}}));
        let failed = serde_json::to_value(Response::failure("no")).unwrap();
        assert_eq!(failed, serde_json::json!({"ok": false, "error": "no"}));
    }
}
