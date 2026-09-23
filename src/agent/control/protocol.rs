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

    #[test]
    fn a_response_carries_a_result_or_an_error_never_both() {
        let ok = serde_json::to_value(Response::success(serde_json::json!({"x": 1}))).unwrap();
        assert_eq!(ok, serde_json::json!({"ok": true, "result": {"x": 1}}));
        let failed = serde_json::to_value(Response::failure("no")).unwrap();
        assert_eq!(failed, serde_json::json!({"ok": false, "error": "no"}));
    }
}
