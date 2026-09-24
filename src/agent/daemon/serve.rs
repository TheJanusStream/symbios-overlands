//! The world's side of the control channel (#1416): answer, on a frame, each
//! request the socket handed over.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError, mpsc};

use bevy::prelude::*;

use super::super::control::protocol::{Response, WorldRequest};
use super::super::control::server::Envelope;
use super::{edit, gifts, look, movement, speech, status, travel, ui};

/// Requests waiting for the world, handed over by the control socket.
#[derive(Resource)]
pub(super) struct ControlInbox(Mutex<Inbox>);

struct Inbox {
    receiver: mpsc::Receiver<Envelope>,
    /// Requests that arrived in time for a frame that had already answered
    /// an edit, in the order they came; the next frame answers them first.
    waiting: VecDeque<Envelope>,
}

impl ControlInbox {
    pub(super) fn new(receiver: mpsc::Receiver<Envelope>) -> Self {
        Self(Mutex::new(Inbox {
            receiver,
            waiting: VecDeque::new(),
        }))
    }

    /// Every request waiting, oldest first.
    fn take(&self) -> VecDeque<Envelope> {
        let mut inbox = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut pending = std::mem::take(&mut inbox.waiting);
        pending.extend(inbox.receiver.try_iter());
        pending
    }

    /// Put back what a frame left unanswered, ahead of anything newer.
    fn put_back(&self, unanswered: VecDeque<Envelope>) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .waiting = unanswered;
    }
}

/// Answer the requests that arrived since the last frame, in order - up to
/// and including one that may write a record, which ends the frame's turn:
/// the undo history takes one step per frame a record changed in, so two
/// edits answered in one frame would come undone together (#1422).
///
/// Exclusive, because a command may need any part of the world; a frame
/// with nothing waiting costs one `try_recv`.
pub(super) fn serve_requests(world: &mut World) {
    let mut pending = world.resource::<ControlInbox>().take();
    while let Some(envelope) = pending.pop_front() {
        let writes = envelope.request.writes_a_record();
        // An interface command that works a control runs over several
        // frames and may write a record on any of them, as an edit does:
        // nothing else that writes one is answered until it has (#1424).
        if writes && ui::acting(world) {
            pending.push_front(envelope);
            break;
        }
        answer(world, envelope.request, envelope.reply);
        if writes {
            break;
        }
    }
    if !pending.is_empty() {
        world.resource::<ControlInbox>().put_back(pending);
    }
}

fn answer(world: &mut World, request: WorldRequest, reply: mpsc::Sender<Response>) {
    let response = match request {
        // A picture is answered frames later, once it has been rendered and
        // written, so it takes the reply with it.
        WorldRequest::Look(spec) => return look::begin(world, spec, reply),
        // So is an interface command, once the passes it needs have run.
        WorldRequest::Ui(request) => return ui::begin(world, request, reply),
        WorldRequest::Status => Response::success(status::snapshot(world)),
        WorldRequest::Stop => {
            // Stopping never waits on edits: the operator's stop is the one
            // command that always works. What it throws away is said.
            let discarded = edit::unsaved(world);
            let saves_in_flight = edit::saving(world);
            if discarded.is_empty() {
                info!("Stopping at the operator's request");
            } else {
                info!(
                    "Stopping at the operator's request; unsaved edits to the {} are discarded",
                    discarded.join(" and the ")
                );
            }
            world.write_message(AppExit::Success);
            Response::success(serde_json::json!({
                "stopping": true,
                "discarded": discarded,
                "saves_in_flight": saves_in_flight,
            }))
        }
        WorldRequest::Say(text) => {
            speech::say(world, &text).map_or_else(Response::failure, Response::success)
        }
        WorldRequest::WalkTo { x, z, run } => movement::walk_to(world, Vec2::new(x, z), run)
            .map_or_else(Response::failure, Response::success),
        WorldRequest::Halt => {
            movement::halt(world).map_or_else(Response::failure, Response::success)
        }
        WorldRequest::Travel {
            room_did,
            label,
            unsaved,
        } => travel::travel(world, room_did, label, unsaved)
            .map_or_else(Response::failure, Response::success),
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
        WorldRequest::Edit(request) => {
            edit::answer(world, request).map_or_else(Response::failure, Response::success)
        }
        WorldRequest::Gift(request) => {
            gifts::answer(world, request).map_or_else(Response::failure, Response::success)
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

#[cfg(test)]
mod one_edit_a_frame_tests {
    use super::super::edit::harness::{AGENT, app_in, placeable_slug};
    use super::*;
    use crate::agent::control::protocol::{EditRecord, EditRequest};
    use crate::ui::undo::RoomUndoHistory;

    fn ask(sender: &mpsc::Sender<Envelope>, request: WorldRequest) -> mpsc::Receiver<Response> {
        let (reply, answer) = mpsc::channel();
        sender.send(Envelope { request, reply }).expect("queued");
        answer
    }

    fn place(x: f32) -> WorldRequest {
        WorldRequest::Edit(EditRequest::Place {
            slug: placeable_slug().to_owned(),
            at: Some([x, 0.0]),
            yaw_deg: None,
        })
    }

    /// THE CASE: the undo history takes one step per frame a record changed
    /// in, so two edits answered in one frame came undone together. Two
    /// edits and a status arrive at once: the first edit ends the first
    /// frame's turn, the second the next, and the status - which writes
    /// nothing - waits its turn behind them. Two steps to undo.
    #[test]
    fn two_edits_waiting_are_two_frames_and_two_steps() {
        let (mut app, _) = app_in(AGENT);
        let (sender, receiver) = mpsc::channel();
        app.world_mut().insert_resource(ControlInbox::new(receiver));
        app.add_systems(Update, serve_requests);
        let steps = |app: &App| app.world().resource::<RoomUndoHistory>().len();
        let before = steps(&app);

        let first = ask(&sender, place(1.0));
        let second = ask(&sender, place(2.0));
        let status = ask(&sender, WorldRequest::Status);
        app.update();
        assert!(first.try_recv().expect("the first edit").ok);
        assert!(second.try_recv().is_err() && status.try_recv().is_err());
        app.update();
        assert!(second.try_recv().expect("the second edit").ok);
        assert!(
            status.try_recv().is_err(),
            "the second edit ended that turn too"
        );
        app.update();
        assert!(status.try_recv().expect("the status").ok);

        assert_eq!(steps(&app), before + 2);
        let undo = ask(
            &sender,
            WorldRequest::Edit(EditRequest::Undo(EditRecord::Room)),
        );
        app.update();
        let undid = undo.try_recv().expect("undone");
        assert!(undid.ok, "{undid:?}");
        let placements = &app
            .world()
            .resource::<crate::state::LiveRoomRecord>()
            .0
            .placements;
        let last = placements.last().expect("a placement");
        assert!(
            matches!(last, crate::pds::Placement::Absolute { transform, .. } if transform.translation.0[0] == 1.0),
            "one undo takes off the second edit only"
        );
    }

    /// A control being worked may write a record on any of its frames, as
    /// an edit does (#1424): an edit that arrives meanwhile waits until the
    /// control is done, and a status, which writes nothing, does not.
    #[test]
    fn an_edit_waits_while_a_control_is_worked() {
        let (mut app, _) = app_in(AGENT);
        let (sender, receiver) = mpsc::channel();
        app.world_mut().insert_resource(ControlInbox::new(receiver));
        app.add_systems(Update, serve_requests);
        ui::working_a_control(app.world_mut());

        let status = ask(&sender, WorldRequest::Status);
        let edit = ask(&sender, place(1.0));
        app.update();

        assert!(status.try_recv().expect("the status").ok);
        assert!(edit.try_recv().is_err(), "the edit waits for the control");
        app.world_mut().remove_resource::<ui::UiWork>();
        app.update();
        assert!(edit.try_recv().expect("the edit, once it is done").ok);
    }
}

#[cfg(test)]
mod stop_tests {
    use super::super::edit::harness::{AGENT, OTHER, app_in, placeable_slug};
    use super::*;
    use crate::agent::control::protocol::EditRequest;

    fn stop(app: &mut App) -> serde_json::Value {
        app.world_mut().init_resource::<Messages<AppExit>>();
        let (sender, receiver) = mpsc::channel();
        app.world_mut().insert_resource(ControlInbox::new(receiver));
        let (reply, answer) = mpsc::channel();
        sender
            .send(Envelope {
                request: WorldRequest::Stop,
                reply,
            })
            .expect("queued");
        serve_requests(app.world_mut());
        answer
            .try_recv()
            .expect("answered")
            .result
            .expect("a result")
    }

    /// Stopping never waits on edits - it is the operator's off switch -
    /// but its answer names what it throws away: the world's unsaved edits
    /// in the agent's own world, none in anyone else's.
    #[test]
    fn stop_says_which_unsaved_edits_it_discards() {
        let (mut app, _) = app_in(AGENT);
        edit::answer(
            app.world_mut(),
            EditRequest::Place {
                slug: placeable_slug().to_owned(),
                at: Some([1.0, 1.0]),
                yaw_deg: None,
            },
        )
        .expect("placed");
        assert_eq!(stop(&mut app)["discarded"], serde_json::json!(["room"]));

        let (mut visiting, _) = app_in(OTHER);
        assert_eq!(stop(&mut visiting)["discarded"], serde_json::json!([]));
    }
}
