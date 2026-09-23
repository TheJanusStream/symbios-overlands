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
    /// Travel to the world of `room_did`, which the operator called `label`.
    Travel {
        room_did: String,
        #[serde(default)]
        label: Option<String>,
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
}

impl WorldRequest {
    /// How long the world may take to answer: a frame for most requests, a
    /// render for a picture.
    pub fn answer_within(&self) -> std::time::Duration {
        use crate::config::agent::{LOOK_ANSWER_TIMEOUT, WORLD_ANSWER_TIMEOUT};
        match self {
            Self::Look(_) => LOOK_ANSWER_TIMEOUT,
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
            Self::Travel { room_did, label } => {
                Route::World(WorldRequest::Travel { room_did, label })
            }
            Self::Look(spec) => Route::World(WorldRequest::Look(spec)),
            Self::Follow { did, distance, run } => Route::World(WorldRequest::Follow {
                did,
                distance: distance.unwrap_or(crate::config::agent::FOLLOW_DISTANCE_M),
                run,
            }),
            Self::Face { did, at } => Route::World(WorldRequest::Face { did, at }),
        }
    }
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

    #[test]
    fn a_response_carries_a_result_or_an_error_never_both() {
        let ok = serde_json::to_value(Response::success(serde_json::json!({"x": 1}))).unwrap();
        assert_eq!(ok, serde_json::json!({"ok": true, "result": {"x": 1}}));
        let failed = serde_json::to_value(Response::failure("no")).unwrap();
        assert_eq!(failed, serde_json::json!({"ok": false, "error": "no"}));
    }
}
