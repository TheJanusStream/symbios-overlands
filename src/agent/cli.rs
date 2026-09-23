//! The agent client's command line (#1413).
//!
//! Every command prints its result to stdout as one JSON object, so an agent
//! reads it the same way every time; progress meant for a person goes to
//! stderr.

use clap::{Args, Parser, Subcommand};

use crate::config::login::{DEFAULT_PDS, DEFAULT_RELAY_HOST};

/// A headless Overlands client that an AI agent drives, signed in as its own
/// account.
#[derive(Parser, Debug)]
#[command(name = "agent", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Sign the agent's own account in once, in a browser, and save its
    /// session so the agent can run with nobody at the keyboard.
    Login(LoginArgs),
    /// List the saved agent sessions.
    Accounts,
    /// Start the agent in the background: resume its saved session and enter
    /// a world as a player. Returns once it takes commands.
    Start(RunArgs),
    /// Run the agent in the foreground, logging to the terminal, until it is
    /// stopped. What `start` runs in the background.
    Run(RunArgs),
    /// Who, where and with whom the running agent is.
    Status(AccountArg),
    /// What has happened since an event, waiting for something to if
    /// nothing has yet.
    Events(EventsArgs),
    /// Stop the running agent. Its saved session is kept.
    Stop(AccountArg),
    /// Say something in the room the agent is in.
    Say(SayArgs),
    /// Walk - or drive - in a straight line to a point on the ground.
    WalkTo(WalkToArgs),
    /// Stop walking.
    Halt(AccountArg),
    /// Travel to another player's world - or `home`.
    Travel(TravelArgs),
}

#[derive(Args, Debug)]
pub struct TravelArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Whose world: a DID, a handle (with or without its @), or `home` for
    /// the agent's own.
    pub to: String,
    /// Wait for the trip to end - arrived or failed - and print how.
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args, Debug)]
pub struct WalkToArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// The point's x, in world metres (as `status` gives positions).
    #[arg(allow_negative_numbers = true)]
    pub x: f32,
    /// The point's z, in world metres.
    #[arg(allow_negative_numbers = true)]
    pub z: f32,
    /// Run rather than walk (hold Shift).
    #[arg(long)]
    pub run: bool,
    /// Wait for the walk to end - arrived, stuck, halted or cut short - and
    /// print how, instead of returning as soon as it starts.
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args, Debug)]
pub struct SayArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// What to say: one line. Newlines become spaces, and a line longer than
    /// the chat window allows is cut.
    pub text: String,
}

/// Which agent a command is for.
#[derive(Args, Debug)]
pub struct AccountArg {
    /// The agent's account, as a handle or DID. Needed only when more than
    /// one session is saved.
    #[arg(long = "account", value_name = "HANDLE_OR_DID")]
    pub name: Option<String>,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// The world to enter, as its owner's DID. The agent's own by default.
    #[arg(long, value_name = "DID", value_parser = room_did)]
    pub room: Option<String>,
    /// Run with no account: a stand-in identity, alone in its seeded world
    /// (or the --room it names). Nobody sees it and nothing it does is
    /// saved - for trying the agent out. Its identity resolves nowhere, so
    /// once it travels away it cannot travel back `home`.
    #[arg(long, conflicts_with = "name")]
    pub offline: bool,
}

#[derive(Args, Debug)]
pub struct EventsArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// The `seq` of the last event already seen; 0 for everything the agent
    /// still keeps.
    #[arg(long, default_value_t = 0)]
    pub since: u64,
    /// Seconds to wait for something to happen when nothing has yet. The
    /// agent waits at most a minute per call.
    #[arg(long, value_name = "SECONDS", default_value_t = 0)]
    pub wait: u64,
}

#[derive(Args, Debug)]
pub struct LoginArgs {
    /// The account's PDS or sign-in server. A bare host gets https://.
    #[arg(long, default_value = DEFAULT_PDS, value_parser = pds_url)]
    pub pds: String,
    /// The relay the agent joins rooms through. A pasted URL is cut down to
    /// its host.
    #[arg(long, default_value = DEFAULT_RELAY_HOST, value_parser = relay_host)]
    pub relay: String,
    /// The agent's account, as a handle or DID. Signing in as any other
    /// account is refused, so a browser still signed in as you cannot hand
    /// the agent your identity.
    #[arg(long, value_name = "HANDLE_OR_DID")]
    pub account: Option<String>,
    /// Only print the sign-in address; do not open a browser.
    #[arg(long)]
    pub no_browser: bool,
}

/// `--pds`: an http(s) URL. A bare host is unambiguous, so it gets
/// `https://` rather than an error.
fn pds_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(format!("empty; the default is {DEFAULT_PDS}"));
    }
    if !trimmed.contains("://") {
        return Ok(format!("https://{trimmed}"));
    }
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        return Ok(trimmed.to_owned());
    }
    Err(format!("{trimmed:?} is not an http(s):// URL"))
}

/// `--relay`: a bare host, which the room address is built around. A pasted
/// scheme and trailing slash are dropped.
fn relay_host(raw: &str) -> Result<String, String> {
    let host = ["wss://", "ws://", "https://", "http://"]
        .iter()
        .fold(raw.trim(), |host, scheme| {
            host.strip_prefix(scheme).unwrap_or(host)
        })
        .trim_end_matches('/');
    if host.is_empty() {
        return Err(format!("empty; the default is {DEFAULT_RELAY_HOST}"));
    }
    if host.contains('/') {
        return Err(format!("{host:?} is not a bare host name"));
    }
    Ok(host.to_owned())
}

/// `--room`: a DID - `did:<method>:<id>`, both parts non-empty.
fn room_did(raw: &str) -> Result<String, String> {
    let did = raw.trim();
    let mut parts = did.splitn(3, ':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("did"), Some(method), Some(id)) if !method.is_empty() && !id.is_empty() => {
            Ok(did.to_owned())
        }
        _ => Err(format!("{did:?} is not a DID (did:plc:... or did:web:...)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory as _;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn login_defaults_to_the_apps_own_pds_and_relay() {
        let cli = Cli::try_parse_from(["agent", "login"]).expect("parses");
        let Command::Login(args) = cli.command else {
            panic!("login");
        };
        assert_eq!(args.pds, DEFAULT_PDS);
        assert_eq!(args.relay, DEFAULT_RELAY_HOST);
        assert_eq!(args.account, None);
        assert!(!args.no_browser);
    }

    /// Offline has no account to name, so naming one is a mistake worth
    /// saying rather than a choice silently ignored.
    #[test]
    fn offline_and_an_account_are_exclusive() {
        assert!(Cli::try_parse_from(["agent", "start", "--offline"]).is_ok());
        assert!(
            Cli::try_parse_from(["agent", "start", "--offline", "--account", "a.test"]).is_err()
        );
    }

    /// A point west or south of the origin has a minus sign, which clap would
    /// otherwise read as the start of a flag.
    #[test]
    fn a_walk_takes_negative_coordinates() {
        let cli =
            Cli::try_parse_from(["agent", "walk-to", "-12.5", "-3", "--wait"]).expect("parses");
        let Command::WalkTo(args) = cli.command else {
            panic!("walk-to");
        };
        assert_eq!((args.x, args.z), (-12.5, -3.0));
        assert!(args.wait && !args.run);
    }

    #[test]
    fn a_bare_pds_host_gets_https() {
        assert_eq!(pds_url("pds.example/").unwrap(), "https://pds.example");
        assert_eq!(
            pds_url("http://localhost:2583").unwrap(),
            "http://localhost:2583"
        );
        assert!(pds_url("   ").is_err());
        assert!(pds_url("ftp://pds.example").is_err());
    }

    #[test]
    fn a_room_is_named_by_a_did() {
        assert_eq!(room_did(" did:plc:abc ").unwrap(), "did:plc:abc");
        assert_eq!(
            room_did("did:web:host.example").unwrap(),
            "did:web:host.example"
        );
        assert!(room_did("alice.bsky.social").is_err());
        assert!(room_did("did:plc:").is_err());
        assert!(room_did("did::abc").is_err());
    }

    #[test]
    fn a_pasted_relay_url_is_cut_to_its_host() {
        assert_eq!(relay_host("wss://relay.example/").unwrap(), "relay.example");
        assert_eq!(relay_host(" relay.example ").unwrap(), "relay.example");
        assert!(relay_host("wss://").is_err());
        assert!(relay_host("relay.example/overlands/did").is_err());
    }
}
