//! Saved agent sessions (#1414): one file per account, written once by
//! `agent login` and kept current by the daemon whenever the session's tokens
//! rotate.
//!
//! A file holds everything a later process needs to act as the account with
//! no browser: the OAuth token set, the DPoP private key those tokens are
//! bound to, the authorization server's metadata (for refreshing), the
//! account's identity and PDS, and the relay host it signed in for. That is
//! the set the browser build keeps in `localStorage`
//! (`oauth::wasm::PersistedSession`), and it sits behind the same trust
//! boundary: anyone who can read the file can act as the account, within the
//! scopes the sign-in granted - the app's own, the Overlands collections and
//! relay tokens, nothing else - until the refresh token expires or is
//! revoked. So the directory is created 0700, every file is written 0600
//! through a private temp file and a rename, and a file that anyone else can
//! read is refused, the way ssh refuses a private key.

use std::fmt;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use proto_blue_oauth::OAuthServerMetadata;
use proto_blue_oauth::types::TokenSet;
use serde::{Deserialize, Serialize};

use super::private_fs::{GROUP_OR_OTHER, ensure_private_dir, write_private};

/// One account's saved session.
#[derive(Serialize, Deserialize, Clone)]
pub struct AgentSession {
    pub did: String,
    pub handle: String,
    /// The account's own PDS, resolved from its DID document at sign-in -
    /// not the entryway the sign-in started at.
    pub pds_url: String,
    /// The relay the session signed in for; the daemon joins rooms there.
    pub relay_host: String,
    pub token_set: TokenSet,
    /// The DPoP private key, as a JWK. The tokens are bound to it.
    pub dpop_jwk: serde_json::Value,
    /// The authorization server's metadata, which names the token endpoint
    /// a refresh goes to.
    pub server_metadata: OAuthServerMetadata,
}

impl AgentSession {
    /// Does `account` name this session's account? See [`names_account`].
    pub fn is_named_by(&self, account: &str) -> bool {
        names_account(account, &self.did, &self.handle)
    }
}

/// Does `account` - a DID, or a handle with or without its `@` - name the
/// account `did` / `handle`? Handles compare without case, as DNS names do.
pub fn names_account(account: &str, did: &str, handle: &str) -> bool {
    let account = account.trim();
    account == did || account.trim_start_matches('@').eq_ignore_ascii_case(handle)
}

/// Why a saved session could not be found, read or written.
#[derive(Debug)]
pub enum SessionFileError {
    /// No config base directory at all: no `XDG_CONFIG_HOME`, `APPDATA` or
    /// `HOME`.
    NoConfigDir,
    /// Nothing is saved for the account named, or nothing at all.
    NoSession {
        account: Option<String>,
    },
    /// Several sessions are saved and no account was named.
    Ambiguous {
        accounts: Vec<String>,
    },
    /// The file can be read by someone other than its owner.
    TooOpen {
        path: PathBuf,
        mode: u32,
    },
    Io {
        path: PathBuf,
        error: io::Error,
    },
    /// The file is not a saved session this build can read.
    Unreadable {
        path: PathBuf,
        error: serde_json::Error,
    },
}

impl fmt::Display for SessionFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigDir => write!(
                f,
                "there is no config directory to keep agent sessions in; set \
                 XDG_CONFIG_HOME or HOME"
            ),
            Self::NoSession { account: None } => write!(
                f,
                "no agent session is saved; sign the agent's account in once with \
                 `agent login`"
            ),
            Self::NoSession {
                account: Some(account),
            } => write!(
                f,
                "no agent session is saved for {account}; `agent accounts` lists the \
                 saved ones, and `agent login` adds one"
            ),
            Self::Ambiguous { accounts } => write!(
                f,
                "several agent sessions are saved ({}); name one with --account",
                accounts.join(", ")
            ),
            Self::TooOpen { path, mode } => write!(
                f,
                "{} can be read by other users (mode {mode:03o}) and it holds the \
                 account's keys; run `chmod 600 {}`",
                path.display(),
                path.display()
            ),
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Unreadable { path, error } => write!(
                f,
                "{} is not a saved agent session this build can read ({error}); sign \
                 the account in again with `agent login`",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SessionFileError {}

/// What [`SessionStore::list`] found: the sessions that loaded, and a
/// problem for each file that did not.
pub struct Listing {
    pub sessions: Vec<(PathBuf, AgentSession)>,
    pub problems: Vec<SessionFileError>,
}

/// The directory saved sessions live in.
pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    /// `<config dir>/agent/sessions/`, beside the app's own settings.
    pub fn platform() -> Result<Self, SessionFileError> {
        use crate::config::agent::{HOME_DIR, SESSIONS_DIR};
        let config = crate::prefs::config_dir().ok_or(SessionFileError::NoConfigDir)?;
        Ok(Self::at(config.join(HOME_DIR).join(SESSIONS_DIR)))
    }

    /// A store in `dir`.
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The file `did`'s session is saved in.
    pub fn path_for(&self, did: &str) -> PathBuf {
        self.dir.join(crate::prefs::account_file_name(did))
    }

    /// Save `session` in its account's file, replacing whatever was there.
    pub fn save(&self, session: &AgentSession) -> Result<PathBuf, SessionFileError> {
        let path = self.path_for(&session.did);
        ensure_private_dir(&self.dir).map_err(|error| SessionFileError::Io {
            path: self.dir.clone(),
            error,
        })?;
        write_session(&path, session)?;
        Ok(path)
    }

    /// Every saved session, sorted by handle, with a problem for each file
    /// that is not one.
    pub fn list(&self) -> Result<Listing, SessionFileError> {
        let mut listing = Listing {
            sessions: Vec::new(),
            problems: Vec::new(),
        };
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(listing),
            Err(error) => {
                return Err(SessionFileError::Io {
                    path: self.dir.clone(),
                    error,
                });
            }
        };
        for entry in entries {
            let path = entry
                .map_err(|error| SessionFileError::Io {
                    path: self.dir.clone(),
                    error,
                })?
                .path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            match read_session(&path) {
                Ok(session) => listing.sessions.push((path, session)),
                Err(problem) => listing.problems.push(problem),
            }
        }
        listing.sessions.sort_by(|a, b| a.1.handle.cmp(&b.1.handle));
        Ok(listing)
    }

    /// The session `account` names, or - with no account named - the only
    /// one saved.
    ///
    /// A named account that is not among the readable sessions reports the
    /// first unreadable file instead when there is one, since that file may
    /// well be the account asked for.
    pub fn find(&self, account: Option<&str>) -> Result<(PathBuf, AgentSession), SessionFileError> {
        let Listing {
            mut sessions,
            mut problems,
        } = self.list()?;
        let not_found = |problems: &mut Vec<SessionFileError>| {
            if problems.is_empty() {
                SessionFileError::NoSession {
                    account: account.map(str::to_owned),
                }
            } else {
                problems.swap_remove(0)
            }
        };
        match account {
            Some(name) => {
                let at = sessions.iter().position(|(_, s)| s.is_named_by(name));
                at.map(|at| sessions.swap_remove(at))
                    .ok_or_else(|| not_found(&mut problems))
            }
            None if sessions.len() > 1 => Err(SessionFileError::Ambiguous {
                accounts: sessions
                    .iter()
                    .map(|(_, s)| format!("@{}", s.handle))
                    .collect(),
            }),
            None => sessions.pop().ok_or_else(|| not_found(&mut problems)),
        }
    }
}

/// Keeps a running agent's session file in step with its tokens (#1414).
///
/// Handed to the game as the session's
/// [`rotation_sink`](crate::oauth::OauthRefreshCtx::rotation_sink), so every
/// refresh - a relay-token mint, a publish - writes the new pair before the
/// call that asked for it returns. The rest of the file never changes while
/// the daemon runs, so it is kept here rather than re-read on every save.
pub struct SessionFileSink {
    path: PathBuf,
    saved: std::sync::Mutex<AgentSession>,
}

impl SessionFileSink {
    pub fn new(path: PathBuf, saved: AgentSession) -> Self {
        Self {
            path,
            saved: std::sync::Mutex::new(saved),
        }
    }
}

impl crate::oauth::TokenSetSink for SessionFileSink {
    fn save(&self, token_set: &TokenSet) -> Result<(), String> {
        let mut saved = self
            .saved
            .lock()
            .map_err(|_| "the session file's lock was poisoned".to_owned())?;
        saved.token_set = token_set.clone();
        write_session(&self.path, &saved).map_err(|e| e.to_string())
    }
}

/// Read one saved session, refusing a file anyone else can read.
pub fn read_session(path: &Path) -> Result<AgentSession, SessionFileError> {
    let io_error = |error| SessionFileError::Io {
        path: path.to_owned(),
        error,
    };
    let mode = fs::metadata(path).map_err(io_error)?.permissions().mode() & 0o777;
    if mode & GROUP_OR_OTHER != 0 {
        return Err(SessionFileError::TooOpen {
            path: path.to_owned(),
            mode,
        });
    }
    let raw = fs::read_to_string(path).map_err(io_error)?;
    serde_json::from_str(&raw).map_err(|error| SessionFileError::Unreadable {
        path: path.to_owned(),
        error,
    })
}

/// Write `session` to `path` - its directory already private - so that the
/// file is never readable by anyone else, not even for a moment, and a
/// crash leaves the old contents or the new, never half of each.
pub fn write_session(path: &Path, session: &AgentSession) -> Result<(), SessionFileError> {
    let json =
        serde_json::to_vec_pretty(session).map_err(|error| SessionFileError::Unreadable {
            path: path.to_owned(),
            error,
        })?;
    write_private(path, &json).map_err(|error| SessionFileError::Io {
        path: path.to_owned(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory for one test, unique across the tests one process
    /// runs in parallel.
    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("symbios-agent-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn session(did: &str, handle: &str) -> AgentSession {
        let server_metadata = serde_json::from_value(serde_json::json!({
            "issuer": "https://pds.example",
            "authorization_endpoint": "https://pds.example/oauth/authorize",
            "token_endpoint": "https://pds.example/oauth/token",
        }))
        .expect("server metadata");
        AgentSession {
            did: did.into(),
            handle: handle.into(),
            pds_url: "https://pds.example".into(),
            relay_host: "relay.example".into(),
            token_set: TokenSet {
                issuer: "https://pds.example".into(),
                sub: did.into(),
                scope: "atproto".into(),
                access_token: "access-1".into(),
                refresh_token: Some("refresh-1".into()),
                token_type: "DPoP".into(),
                expires_at: Some("2099-01-01T00:00:00Z".into()),
                aud: Some("https://pds.example".into()),
            },
            dpop_jwk: serde_json::json!({"kty": "EC", "crv": "P-256"}),
            server_metadata,
        }
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).expect("exists").permissions().mode() & 0o777
    }

    /// The account's keys never sit in a file or directory anyone else can
    /// read - checked on what was actually created, not on the flags asked
    /// for, since a umask can only take bits away and this has to hold
    /// whatever the umask is.
    #[test]
    fn a_saved_session_is_private_to_its_owner() {
        let dir = scratch("private");
        let store = SessionStore::at(dir.join("sessions"));

        let path = store
            .save(&session("did:plc:agent", "agent.test"))
            .expect("saved");

        assert_eq!(mode(&path), 0o600, "the file");
        assert_eq!(mode(&dir.join("sessions")), 0o700, "the directory");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A directory that already existed with looser permissions is taken
    /// back to owner-only before a session is written into it.
    #[test]
    fn a_loose_sessions_directory_is_tightened() {
        let dir = scratch("tighten");
        let sessions = dir.join("sessions");
        fs::create_dir_all(&sessions).expect("created");
        fs::set_permissions(&sessions, fs::Permissions::from_mode(0o755)).expect("loosened");

        SessionStore::at(sessions.clone())
            .save(&session("did:plc:agent", "agent.test"))
            .expect("saved");

        assert_eq!(mode(&sessions), 0o700);
        let _ = fs::remove_dir_all(&dir);
    }

    /// What `login` writes is what `run` reads back, token set included.
    #[test]
    fn a_saved_session_reads_back_as_it_was_written() {
        let dir = scratch("roundtrip");
        let store = SessionStore::at(dir.clone());
        let saved = session("did:plc:agent", "agent.test");

        let path = store.save(&saved).expect("saved");
        let read = read_session(&path).expect("read back");

        assert_eq!(read.did, saved.did);
        assert_eq!(read.handle, saved.handle);
        assert_eq!(read.pds_url, saved.pds_url);
        assert_eq!(read.relay_host, saved.relay_host);
        assert_eq!(read.token_set.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(read.dpop_jwk, saved.dpop_jwk);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A copy someone left group- or world-readable is refused rather than
    /// used, and the refusal says how to fix it.
    #[test]
    fn a_session_file_others_can_read_is_refused() {
        let dir = scratch("too-open");
        let store = SessionStore::at(dir.clone());
        let path = store
            .save(&session("did:plc:agent", "agent.test"))
            .expect("saved");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("loosened");

        let err = read_session(&path).err().expect("refused");

        assert!(
            matches!(err, SessionFileError::TooOpen { mode: 0o644, .. }),
            "{err}"
        );
        assert!(err.to_string().contains("chmod 600"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Saving again replaces the account's file rather than adding a second
    /// one, and leaves no temp file behind.
    #[test]
    fn saving_again_replaces_the_accounts_file() {
        let dir = scratch("replace");
        let store = SessionStore::at(dir.clone());
        let mut saved = session("did:plc:agent", "agent.test");
        store.save(&saved).expect("first save");
        saved.token_set.refresh_token = Some("refresh-2".into());

        let path = store.save(&saved).expect("second save");

        let files: Vec<_> = fs::read_dir(&dir)
            .expect("listed")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(files.len(), 1, "one file per account, no temp: {files:?}");
        let read = read_session(&path).expect("read back");
        assert_eq!(read.token_set.refresh_token.as_deref(), Some("refresh-2"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_account_is_named_by_its_did_or_its_handle() {
        let saved = session("did:plc:agent", "Agent.Test");

        assert!(saved.is_named_by("did:plc:agent"));
        assert!(saved.is_named_by("agent.test"), "handles ignore case");
        assert!(saved.is_named_by("@agent.test"), "with the @ people type");
        assert!(saved.is_named_by("  agent.test "), "and stray spaces");
        assert!(!saved.is_named_by("did:plc:someone-else"));
        assert!(!saved.is_named_by("agent"), "a prefix is not a name");
    }

    /// With one session saved, no name is needed; with two, one is.
    #[test]
    fn find_picks_the_only_session_and_asks_when_there_are_several() {
        let dir = scratch("find");
        let store = SessionStore::at(dir.clone());
        assert!(matches!(
            store.find(None),
            Err(SessionFileError::NoSession { account: None })
        ));

        store
            .save(&session("did:plc:one", "one.test"))
            .expect("saved");
        let (_, only) = store.find(None).expect("the only one");
        assert_eq!(only.did, "did:plc:one");

        store
            .save(&session("did:plc:two", "two.test"))
            .expect("saved");
        match store.find(None) {
            Err(SessionFileError::Ambiguous { accounts }) => {
                assert_eq!(accounts, ["@one.test", "@two.test"]);
            }
            other => panic!("expected Ambiguous, got {:?}", other.map(|(p, _)| p)),
        }
        let (_, named) = store.find(Some("@two.test")).expect("named");
        assert_eq!(named.did, "did:plc:two");
        assert!(matches!(
            store.find(Some("three.test")),
            Err(SessionFileError::NoSession { account: Some(_) })
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    /// A rotation lands in the file with everything else in it unchanged,
    /// and the file stays private - the sink writes through the same path
    /// `login` does.
    #[test]
    fn the_sink_saves_a_rotation_over_the_old_tokens() {
        use crate::oauth::TokenSetSink as _;
        let dir = scratch("sink");
        let store = SessionStore::at(dir.clone());
        let saved = session("did:plc:agent", "agent.test");
        let path = store.save(&saved).expect("saved");
        let sink = SessionFileSink::new(path.clone(), saved.clone());
        let mut rotated = saved.token_set.clone();
        rotated.access_token = "access-2".into();
        rotated.refresh_token = Some("refresh-2".into());

        sink.save(&rotated).expect("the rotation is saved");

        let read = read_session(&path).expect("read back");
        assert_eq!(read.token_set.access_token, "access-2");
        assert_eq!(read.token_set.refresh_token.as_deref(), Some("refresh-2"));
        assert_eq!(
            read.dpop_jwk, saved.dpop_jwk,
            "the key the tokens are bound to"
        );
        assert_eq!(read.relay_host, saved.relay_host);
        assert_eq!(mode(&path), 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A broken file shows up as a problem beside the sessions that load,
    /// rather than hiding them.
    #[test]
    fn an_unreadable_file_is_listed_as_a_problem() {
        let dir = scratch("problem");
        let store = SessionStore::at(dir.clone());
        store
            .save(&session("did:plc:good", "good.test"))
            .expect("saved");
        let broken = dir.join("broken.json");
        write_private(&broken, b"{ not a session").expect("written");

        let listing = store.list().expect("listed");

        assert_eq!(listing.sessions.len(), 1);
        assert_eq!(listing.problems.len(), 1);
        assert!(
            matches!(&listing.problems[0], SessionFileError::Unreadable { path, .. } if *path == broken)
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
