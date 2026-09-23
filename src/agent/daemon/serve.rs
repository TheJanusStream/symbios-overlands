//! The world's side of the control channel (#1416): answer, on a frame, each
//! request the socket handed over.

use std::sync::{Mutex, PoisonError, mpsc};

use bevy::prelude::*;

use super::super::control::protocol::{Response, WorldRequest};
use super::super::control::server::Envelope;
use super::{look, movement, speech, status, travel};

/// Requests waiting for the world, handed over by the control socket.
#[derive(Resource)]
pub(super) struct ControlInbox(Mutex<mpsc::Receiver<Envelope>>);

impl ControlInbox {
    pub(super) fn new(receiver: mpsc::Receiver<Envelope>) -> Self {
        Self(Mutex::new(receiver))
    }
}

/// Answer every request that arrived since the last frame.
///
/// Exclusive, because a command may need any part of the world; a frame
/// with nothing waiting costs one `try_recv`.
pub(super) fn serve_requests(world: &mut World) {
    let pending: Vec<Envelope> = {
        let inbox = world.resource::<ControlInbox>();
        let receiver = inbox.0.lock().unwrap_or_else(PoisonError::into_inner);
        receiver.try_iter().collect()
    };
    for Envelope { request, reply } in pending {
        answer(world, request, reply);
    }
}

fn answer(world: &mut World, request: WorldRequest, reply: mpsc::Sender<Response>) {
    let response = match request {
        // A picture is answered frames later, once it has been rendered and
        // written, so it takes the reply with it.
        WorldRequest::Look(spec) => return look::begin(world, spec, reply),
        WorldRequest::Status => Response::success(status::snapshot(world)),
        WorldRequest::Stop => {
            info!("Stopping at the operator's request");
            world.write_message(AppExit::Success);
            Response::success(serde_json::json!({ "stopping": true }))
        }
        WorldRequest::Say(text) => {
            speech::say(world, &text).map_or_else(Response::failure, Response::success)
        }
        WorldRequest::WalkTo { x, z, run } => movement::walk_to(world, Vec2::new(x, z), run)
            .map_or_else(Response::failure, Response::success),
        WorldRequest::Halt => {
            movement::halt(world).map_or_else(Response::failure, Response::success)
        }
        WorldRequest::Travel { room_did, label } => {
            travel::travel(world, room_did, label).map_or_else(Response::failure, Response::success)
        }
        WorldRequest::Follow { did, distance, run } => movement::follow(world, did, distance, run)
            .map_or_else(Response::failure, Response::success),
        WorldRequest::Face { did, at } => {
            let target = match (did, at) {
                (Some(did), None) => Ok(movement::FaceTarget::Peer(did)),
                (None, Some(at)) => Ok(movement::FaceTarget::Point(Vec2::from_array(at))),
                _ => Err("face takes a player or a point, not both and not neither".to_owned()),
            };
            target
                .and_then(|target| movement::face(world, target))
                .map_or_else(Response::failure, Response::success)
        }
    };
    // INTENTIONAL: the client may have stopped waiting; there is nobody left
    // to tell.
    let _ = reply.send(response);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_inbox() -> (World, mpsc::Sender<Envelope>) {
        let (sender, receiver) = mpsc::channel();
        let mut world = World::new();
        world.init_resource::<Messages<AppExit>>();
        world.insert_resource(State::new(crate::state::AppState::Login));
        world.insert_resource(ControlInbox::new(receiver));
        (world, sender)
    }

    fn ask(sender: &mpsc::Sender<Envelope>, request: WorldRequest) -> mpsc::Receiver<Response> {
        let (reply, answer) = mpsc::channel();
        sender.send(Envelope { request, reply }).expect("queued");
        answer
    }

    /// Every request waiting when the frame runs is answered in that frame.
    #[test]
    fn every_waiting_request_is_answered_in_one_frame() {
        let (mut world, sender) = world_with_inbox();
        let first = ask(&sender, WorldRequest::Status);
        let second = ask(&sender, WorldRequest::Status);

        serve_requests(&mut world);

        for answer in [first, second] {
            let response = answer.try_recv().expect("answered this frame");
            assert!(response.ok, "{response:?}");
        }
    }

    /// `stop` ends the app the way closing a window does - with `AppExit` -
    /// and never through the game's logout, which would revoke the session.
    #[test]
    fn stop_writes_app_exit() {
        let (mut world, sender) = world_with_inbox();
        let answer = ask(&sender, WorldRequest::Stop);

        serve_requests(&mut world);

        assert!(answer.try_recv().expect("answered").ok);
        let exits = world.resource::<Messages<AppExit>>();
        let mut cursor = exits.get_cursor();
        assert_eq!(
            cursor.read(exits).cloned().collect::<Vec<_>>(),
            [AppExit::Success]
        );
    }
}
