//! The CLI's end of the control socket (#1416): send one request to a
//! running daemon and read its one answer.

use std::io::{self, BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::config::agent::WORLD_ANSWER_TIMEOUT;

use super::protocol::{Request, Response};

/// How much longer than the daemon's own bound the client waits, so the
/// daemon's timeout - which says what timed out - is the one that fires.
const MARGIN: Duration = Duration::from_secs(5);

/// Send `request` to the daemon on `path` and return its answer.
pub fn call(path: &Path, request: &Request) -> Result<Response, String> {
    let mut stream = UnixStream::connect(path).map_err(|e| not_running(path, &e))?;
    stream
        .set_read_timeout(Some(answer_bound(request)))
        .map_err(|e| format!("the connection: {e}"))?;
    let mut line = serde_json::to_vec(request).map_err(|e| format!("encoding: {e}"))?;
    line.push(b'\n');
    if let Err(e) = stream.write_all(&line) {
        // The daemon stops reading a request past its bound and answers why
        // before it hangs up, so a send cut short may still have an answer
        // waiting: read it rather than report the bare broken pipe (#1511).
        return match e.kind() {
            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset => {
                read_answer(stream).map_err(|_| format!("sending the request: {e}"))
            }
            _ => Err(format!("sending the request: {e}")),
        };
    }
    read_answer(stream)
}

/// The daemon's one answer on `stream`.
fn read_answer(stream: UnixStream) -> Result<Response, String> {
    let mut answer = String::new();
    BufReader::new(stream)
        .read_line(&mut answer)
        .map_err(|e| format!("reading the answer: {e}"))?;
    if answer.is_empty() {
        return Err("the agent closed the connection without answering".to_owned());
    }
    serde_json::from_str(&answer).map_err(|e| format!("an answer this CLI cannot read: {e}"))
}

/// How long to wait for the answer to `request`.
fn answer_bound(request: &Request) -> Duration {
    let waits = match request {
        Request::Events { wait_secs, .. } => {
            Duration::from_secs(*wait_secs).min(crate::config::agent::MAX_EVENT_WAIT)
        }
        Request::Look(_) => crate::config::agent::LOOK_ANSWER_TIMEOUT,
        _ => WORLD_ANSWER_TIMEOUT,
    };
    waits + MARGIN
}

fn not_running(path: &Path, error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => {
            "no agent is running for this account; start one with `agent start`".to_owned()
        }
        _ => format!("reaching the agent at {}: {error}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_daemon_says_how_to_start_one() {
        let path = std::env::temp_dir().join(format!("sa-none-{}.sock", std::process::id()));
        let err = call(&path, &Request::Status).expect_err("nobody is listening");
        assert!(err.contains("agent start"), "{err}");
    }

    /// A daemon that stops reading a request past its bound answers why and
    /// hangs up while the send is still going: the CLI reads that answer
    /// instead of reporting "Broken pipe" (#1511).
    #[test]
    fn a_request_refused_mid_send_is_answered_with_the_daemons_reason() {
        let dir = std::env::temp_dir().join(format!("sa-cut-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        let path = dir.join("agent.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bound");
        // the stand-in reads a little, answers why it stopped, and hangs up
        // with the rest unread - as the daemon does past MAX_REQUEST_BYTES
        let daemon = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a connection");
            let mut first = [0_u8; 16];
            let _ = io::Read::read(&mut stream, &mut first);
            let mut line =
                serde_json::to_vec(&Response::failure("the request is too long")).expect("encoded");
            line.push(b'\n');
            stream.write_all(&line).expect("answered");
        });
        // far past what the socket buffers, so the send is still going when
        // the stand-in hangs up
        let request = Request::RoomSet {
            pointer: "/generators".into(),
            value: serde_json::Value::String("a".repeat(8 << 20)),
        };

        let response = call(&path, &request).expect("the refusal, read");

        daemon.join().expect("the stand-in finished");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!response.ok);
        assert_eq!(response.error.as_deref(), Some("the request is too long"));
    }

    #[test]
    fn an_events_wait_is_bounded_by_what_the_daemon_will_wait() {
        let bound = answer_bound(&Request::Events {
            since: 0,
            wait_secs: u64::MAX,
        });
        assert_eq!(bound, crate::config::agent::MAX_EVENT_WAIT + MARGIN);
    }
}
