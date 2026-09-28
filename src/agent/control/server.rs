//! The daemon's end of the control socket (#1416).
//!
//! One thread accepts; each connection gets a short-lived thread of its own,
//! so a client waiting on `events` never holds up one asking for `status`.
//! A request for the world goes to the daemon over a channel, with a channel
//! of its own for the answer.

use std::fs;
use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use crate::config::agent::{MAX_EVENT_WAIT, MAX_REQUEST_BYTES, REQUEST_READ_TIMEOUT};

use super::events::EventLog;
use super::protocol::{Request, Response, Route, WorldRequest};

/// A request for the world, and where its answer goes.
pub struct Envelope {
    pub request: WorldRequest,
    pub reply: mpsc::Sender<Response>,
}

/// A bound control socket. Dropping it removes the socket file, so the next
/// daemon for the account does not find a stale one.
pub struct ControlSocket {
    path: PathBuf,
}

impl Drop for ControlSocket {
    fn drop(&mut self) {
        // INTENTIONAL: best effort - a socket left behind is found stale and
        // cleared by the next daemon's `listen`.
        let _ = fs::remove_file(&self.path);
    }
}

/// Bind `path` and serve it on background threads: world requests go to
/// `requests`, `events` are answered from `events`.
///
/// Refuses to start beside a daemon that is already answering on `path` -
/// the relay would let only one of them into a room anyway - and clears a
/// socket file a dead one left behind.
pub fn listen(
    path: &Path,
    requests: mpsc::Sender<Envelope>,
    events: Arc<EventLog>,
) -> Result<ControlSocket, String> {
    let dir = path.parent().ok_or("the socket path has no directory")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    clear_stale_socket(path)?;
    let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let socket = ControlSocket {
        path: path.to_owned(),
    };
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    thread::Builder::new()
        .name("agent-control".into())
        .spawn(move || accept(listener, requests, events))
        .map_err(|e| format!("starting the control thread: {e}"))?;
    Ok(socket)
}

/// Is a daemon answering on `path`?
pub fn is_answering(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}

fn clear_stale_socket(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if is_answering(path) {
        return Err(format!(
            "an agent is already running for this account (it answers on {}); \
             `agent stop` ends it",
            path.display()
        ));
    }
    fs::remove_file(path).map_err(|e| format!("clearing the stale {}: {e}", path.display()))
}

fn accept(listener: UnixListener, requests: mpsc::Sender<Envelope>, events: Arc<EventLog>) {
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                bevy::log::warn!("control socket: accepting a connection failed: {e}");
                continue;
            }
        };
        let requests = requests.clone();
        let events = Arc::clone(&events);
        let spawned = thread::Builder::new()
            .name("agent-connection".into())
            .spawn(move || serve(stream, &requests, &events));
        if let Err(e) = spawned {
            bevy::log::warn!("control socket: no thread for a connection: {e}");
        }
    }
}

fn serve(stream: UnixStream, requests: &mpsc::Sender<Envelope>, events: &EventLog) {
    let response = match read_request(&stream) {
        Ok(request) => answer(request, requests, events),
        Err(e) => Response::failure(e),
    };
    // INTENTIONAL: a client that hung up before its answer has nothing to be
    // told, and the daemon has nothing to do about it.
    let _ = write_response(stream, &response);
}

fn answer(request: Request, requests: &mpsc::Sender<Envelope>, events: &EventLog) -> Response {
    match request.route() {
        Route::Events { since, wait_secs } => {
            let wait = Duration::from_secs(wait_secs).min(MAX_EVENT_WAIT);
            match serde_json::to_value(events.after(since, wait)) {
                Ok(batch) => Response::success(batch),
                Err(e) => Response::failure(format!("encoding the events: {e}")),
            }
        }
        Route::World(request) => ask_the_world(request, requests),
    }
}

/// Hand `request` to the daemon and wait for its answer, which comes on the
/// daemon's next frame (a picture, a few frames later) - or never, if the
/// daemon is shutting down.
fn ask_the_world(request: WorldRequest, requests: &mpsc::Sender<Envelope>) -> Response {
    let within = request.answer_within();
    let (reply, answer) = mpsc::channel();
    if requests.send(Envelope { request, reply }).is_err() {
        return Response::failure("the agent is shutting down");
    }
    answer.recv_timeout(within).unwrap_or_else(|_| {
        Response::failure(format!(
            "the agent did not answer within {} s",
            within.as_secs()
        ))
    })
}

/// Read one request line, bounded in size and in time.
///
/// Read as bytes, and held to the bound before it is read as text: a line
/// cut at the bound can end inside a character, and read as text it would
/// be refused for that instead of for its length (#1511).
fn read_request(stream: &UnixStream) -> Result<Request, String> {
    stream
        .set_read_timeout(Some(REQUEST_READ_TIMEOUT))
        .map_err(|e| format!("the connection: {e}"))?;
    let mut line = Vec::new();
    BufReader::new(stream.take(MAX_REQUEST_BYTES))
        .read_until(b'\n', &mut line)
        .map_err(|e| match e.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
                "no request arrived in time".to_owned()
            }
            _ => format!("reading the request: {e}"),
        })?;
    if line.last() != Some(&b'\n') && line.len() as u64 >= MAX_REQUEST_BYTES {
        return Err(format!(
            "the request is longer than {MAX_REQUEST_BYTES} bytes"
        ));
    }
    let line = String::from_utf8(line).map_err(|e| format!("the request is not UTF-8: {e}"))?;
    serde_json::from_str(line.trim()).map_err(|e| format!("not a request this agent knows: {e}"))
}

fn write_response(mut stream: UnixStream, response: &Response) -> io::Result<()> {
    let mut line = serde_json::to_vec(response).map_err(io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh socket path for one test, short enough for a Unix socket.
    fn socket(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sa-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("agent.sock")
    }

    fn call(path: &Path, line: &str) -> Response {
        let mut stream = UnixStream::connect(path).expect("connected");
        stream.write_all(line.as_bytes()).expect("sent");
        let mut answer = String::new();
        BufReader::new(stream)
            .read_line(&mut answer)
            .expect("answered");
        serde_json::from_str(&answer).expect("a response")
    }

    /// A world request crosses to the daemon's side of the channel and its
    /// answer comes back on the same connection.
    #[test]
    fn a_world_request_is_answered_by_the_other_end_of_the_channel() {
        let path = socket("world");
        let (requests, inbox) = mpsc::channel::<Envelope>();
        let _socket =
            listen(&path, requests, Arc::new(EventLog::new(4, "t".into()))).expect("listening");
        let daemon = thread::spawn(move || {
            let envelope = inbox.recv().expect("a request");
            assert_eq!(envelope.request, WorldRequest::Status);
            envelope
                .reply
                .send(Response::success(serde_json::json!({"here": true})))
                .expect("replied");
        });

        let response = call(&path, "{\"command\":\"status\"}\n");

        assert_eq!(
            response,
            Response::success(serde_json::json!({"here": true}))
        );
        daemon.join().expect("the daemon side finished");
    }

    #[test]
    fn events_are_answered_without_the_world() {
        let path = socket("events");
        let (requests, _inbox) = mpsc::channel::<Envelope>();
        let events = Arc::new(EventLog::new(4, "t".into()));
        events.push(super::super::events::EventKind::EnteredWorld {
            room_did: "did:plc:home".into(),
        });
        let _socket = listen(&path, requests, events).expect("listening");

        let response = call(
            &path,
            "{\"command\":\"events\",\"since\":0,\"wait_secs\":0}\n",
        );

        assert!(response.ok, "{response:?}");
        let result = response.result.expect("a batch");
        assert_eq!(result["events"][0]["kind"], "entered_world");
        assert_eq!(result["next"], 1);
    }

    /// A `room set` of a whole world's generators - Ashmere's were 1.5 MB in
    /// session 885 - is read whole and reaches the daemon (#1511): the line
    /// was cut at 1 MiB, and the set refused.
    #[test]
    fn a_room_set_past_a_mebibyte_reaches_the_daemon_whole() {
        let path = socket("large");
        let (requests, inbox) = mpsc::channel::<Envelope>();
        let _socket =
            listen(&path, requests, Arc::new(EventLog::new(4, "t".into()))).expect("listening");
        let value = "a".repeat(3 << 20);
        let daemon = thread::spawn(move || {
            let Ok(envelope) = inbox.recv_timeout(Duration::from_secs(20)) else {
                return 0;
            };
            let WorldRequest::Edit(super::super::protocol::EditRequest::Set { value, .. }) =
                &envelope.request
            else {
                panic!("not a set: {:?}", envelope.request);
            };
            let got = value.as_str().map_or(0, str::len);
            envelope
                .reply
                .send(Response::success(serde_json::json!({"set": true})))
                .expect("replied");
            got
        });

        let line = format!(
            "{{\"command\":\"room_set\",\"pointer\":\"/generators\",\"value\":\"{value}\"}}\n"
        );
        let response = call(&path, &line);

        assert!(response.ok, "{response:?}");
        assert_eq!(daemon.join().expect("the daemon side finished"), 3 << 20);
    }

    /// A request past the bound is refused for its length, through the
    /// real daemon's socket and the real client (#1511): the daemon reads
    /// up to the bound, answers why and hangs up with the rest unsent, and
    /// the client reads that answer rather than report the broken pipe.
    #[test]
    fn a_request_past_the_bound_is_refused_with_the_bound() {
        let path = socket("bound");
        let (requests, _inbox) = mpsc::channel::<Envelope>();
        let _socket =
            listen(&path, requests, Arc::new(EventLog::new(4, "t".into()))).expect("listening");
        let request = Request::RoomSet {
            pointer: "/generators".into(),
            value: serde_json::Value::String("a".repeat(MAX_REQUEST_BYTES as usize + (4 << 20))),
        };

        let response = super::super::client::call(&path, &request).expect("the refusal, read");

        assert!(!response.ok);
        assert_eq!(
            response.error.as_deref(),
            Some(format!("the request is longer than {MAX_REQUEST_BYTES} bytes").as_str())
        );
    }

    /// A request cut at the bound in the middle of a character is refused
    /// for its length too, not as text that is not UTF-8 (#1511): a JSON
    /// string carries a non-ASCII character as its bytes, and nothing puts
    /// the bound between two characters.
    #[test]
    fn a_request_cut_inside_a_character_is_refused_as_too_long() {
        let path = socket("midchar");
        let (requests, _inbox) = mpsc::channel::<Envelope>();
        let _socket =
            listen(&path, requests, Arc::new(EventLog::new(4, "t".into()))).expect("listening");
        let head = "{\"command\":\"room_set\",\"pointer\":\"/generators\",\"value\":\"";
        // The first of U+00E9's two bytes is the last byte the daemon reads.
        let lead = MAX_REQUEST_BYTES as usize - head.len() - 1;
        let line = format!("{head}{}\u{e9}\"}}\n", "a".repeat(lead));
        assert_eq!(line.as_bytes()[MAX_REQUEST_BYTES as usize - 1], 0xc3);

        let mut stream = UnixStream::connect(&path).expect("connected");
        // INTENTIONAL: the daemon may hang up before the last bytes are sent.
        let _ = stream.write_all(line.as_bytes());
        let mut answer = String::new();
        BufReader::new(stream)
            .read_line(&mut answer)
            .expect("answered");
        let response: Response = serde_json::from_str(&answer).expect("a response");

        assert!(!response.ok);
        assert_eq!(
            response.error.as_deref(),
            Some(format!("the request is longer than {MAX_REQUEST_BYTES} bytes").as_str())
        );
    }

    #[test]
    fn a_request_it_does_not_know_is_refused_with_a_reason() {
        let path = socket("unknown");
        let (requests, _inbox) = mpsc::channel::<Envelope>();
        let _socket =
            listen(&path, requests, Arc::new(EventLog::new(4, "t".into()))).expect("listening");

        let response = call(&path, "{\"command\":\"fly\"}\n");

        assert!(!response.ok);
        assert!(
            response
                .error
                .as_deref()
                .unwrap_or("")
                .contains("not a request"),
            "{response:?}"
        );
    }

    /// Only the owner can reach the socket: private file, private directory.
    #[test]
    fn the_socket_is_private_to_its_owner() {
        let path = socket("private");
        let (requests, _inbox) = mpsc::channel::<Envelope>();
        let _socket =
            listen(&path, requests, Arc::new(EventLog::new(4, "t".into()))).expect("listening");

        let mode = |p: &Path| fs::metadata(p).expect("exists").permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().expect("a directory")), 0o700);
    }

    /// A second daemon for the same account is refused while the first one
    /// answers; once the first is gone, its socket file is cleared, not
    /// tripped over.
    #[test]
    fn one_daemon_per_account_and_a_dead_ones_socket_is_cleared() {
        let path = socket("single");
        let (requests, _inbox) = mpsc::channel::<Envelope>();
        let first = listen(
            &path,
            requests.clone(),
            Arc::new(EventLog::new(4, "t".into())),
        )
        .expect("the first daemon listens");

        let second = listen(
            &path,
            requests.clone(),
            Arc::new(EventLog::new(4, "t".into())),
        );
        assert!(
            second
                .as_ref()
                .err()
                .is_some_and(|e| e.contains("already running")),
            "{:?}",
            second.err()
        );
        drop(first);

        // A stale file where a dead daemon's socket was: bound, then closed
        // without being unlinked, which is what a killed process leaves. (A
        // listener that is merely never accepted from still answers, since
        // the kernel queues the connection.)
        drop(UnixListener::bind(&path).expect("a leftover socket file"));
        assert!(path.exists(), "the file outlives its listener");
        let third = listen(&path, requests, Arc::new(EventLog::new(4, "t".into())));
        assert!(third.is_ok(), "{:?}", third.err());
    }
}
