//! The agent client's command line (#1413).
//!
//! Every command prints its result to stdout as one JSON object, so an agent
//! reads it the same way every time; progress meant for a person goes to
//! stderr.

use clap::{Args, Parser, Subcommand};

use crate::config::login::{DEFAULT_PDS, DEFAULT_RELAY_HOST};

use super::control::protocol::{EditRecord, LookView};

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
    Run(DaemonArgs),
    /// Who, where and with whom the running agent is.
    Status(AccountArg),
    /// What has happened since an event, waiting for something to if
    /// nothing has yet.
    Events(EventsArgs),
    /// Stop the running agent. Its saved session is kept.
    Stop(AccountArg),
    /// Say something in the room the agent is in.
    Say(SayArgs),
    /// Walk - or drive - in a straight line to a point on the ground; a body
    /// that flies flies there and lands on it.
    WalkTo(WalkToArgs),
    /// Stop moving. A body that flies comes straight down first, and the
    /// movement ends once it is down.
    Halt(AccountArg),
    /// Travel to another player's world - or `home`.
    Travel(TravelArgs),
    /// Take a picture of what the agent sees, write it to a PNG and print
    /// where. Nothing is drawn while nobody asks.
    Look(LookArgs),
    /// Follow another player in this world, keeping near them until halted,
    /// until they leave, or until the agent travels. A body that flies
    /// escorts them low, and lands beside them once they stand still.
    Follow(FollowArgs),
    /// Turn to face another player, or a point on the ground - on the spot,
    /// in the air or on the ground, for a body that flies.
    Face(FaceArgs),
    /// List the things placed in the agent's own world, by index, each
    /// where it is drawn.
    Placements(PlacementsArgs),
    /// List the catalogue of things that can be placed - those matching a
    /// search, when given.
    Catalogue(CatalogueArgs),
    /// Put a catalogue item down in the agent's own world. Everyone there
    /// sees it at once; it stays only once saved.
    Place(PlaceArgs),
    /// Move something placed in the agent's own world to another point,
    /// keeping its height above the ground.
    Move(MoveArgs),
    /// Take something placed out of the agent's own world.
    Remove(RemoveArgs),
    /// Read or write the agent's world record as JSON, as the World
    /// Editor's Raw JSON tab does.
    Room(RecordJsonArgs),
    /// Read or write the agent's avatar as JSON.
    Avatar(RecordJsonArgs),
    /// Undo the last edit to the world - or to the avatar.
    Undo(RecordArg),
    /// Redo the last edit undone.
    Redo(RecordArg),
    /// Throw away the unsaved edits to the world - or to the avatar - and
    /// go back to what was last saved.
    Revert(RecordArg),
    /// Save the world - or the avatar, or the inventory - to the agent's
    /// account, where everyone who visits sees it. Refused unless the agent
    /// was started with --allow-save.
    Save(SaveArgs),
    /// List what the agent's inventory holds.
    Inventory(AccountArg),
    /// Put something into the inventory: a thing in the agent's own world,
    /// by the name `placements` gives it, or a catalogue entry, by its slug.
    Stash(StashArgs),
    /// Take an item out of the inventory.
    Unstash(ItemArgs),
    /// Put an inventory item on the avatar.
    Wear(ItemArgs),
    /// Take a worn item off the avatar.
    TakeOff(ItemArgs),
    /// Offer a gift to another player in the agent's world, or answer one
    /// the admin offered the agent. Anyone else's offer is declined the
    /// moment it arrives.
    Gift(GiftArgs),
    /// Read and work the game's own interface, window by window, as a
    /// person does. `agent ui` alone says what is open; `agent ui show
    /// <window>` lists a window's controls by the paths the rest take.
    Ui(UiArgs),
}

#[derive(Args, Debug)]
pub struct UiArgs {
    /// The agent's account, as a handle or DID. Needed only when more than
    /// one session is saved.
    #[arg(long = "account", value_name = "HANDLE_OR_DID", global = true)]
    pub account: Option<String>,
    /// With `agent ui` alone: a PNG of the whole interface as a person
    /// sees it - drawn only when asked - and where it was written.
    #[arg(long)]
    pub picture: bool,
    #[command(subcommand)]
    pub action: Option<UiAction>,
}

#[derive(Subcommand, Debug)]
pub enum UiAction {
    /// List a window's controls and words - or the dialog the agent's
    /// click raised, or `toasts` - each by its path.
    Show {
        /// The window, by its title (World Editor) or key (world_editor).
        window: String,
        /// Also a PNG of the window as a person sees it, and where it was
        /// written: what the listing cannot carry - pictures, colours,
        /// what overlaps what.
        #[arg(long)]
        picture: bool,
    },
    /// Open a window, as its toolbar button does, and list it.
    Open {
        /// The window, by its title (World Editor) or key (world_editor).
        window: String,
    },
    /// Close a window. Every open window costs its drawing every frame.
    Close {
        /// The window, by its title or key.
        window: String,
    },
    /// Click a button, a checkbox, a tab, a section's header, or a row of
    /// a list.
    Click {
        /// The control's path, as `ui show` lists it (`Avatar > Re-roll`),
        /// or enough of its end to name one control.
        path: String,
    },
    /// Type into a text field, replacing what it holds.
    Type {
        /// The field's path, as `ui show` lists it.
        path: String,
        /// What to type.
        text: String,
        /// Press Enter after, as a person does to apply a field.
        #[arg(long)]
        enter: bool,
    },
    /// Set a slider or a number field.
    Set {
        /// The control's path, as `ui show` lists it.
        path: String,
        /// The value. The control holds it to its own range.
        #[arg(allow_hyphen_values = true)]
        value: f64,
    },
    /// Pick an option from a combo box or a menu.
    Choose {
        /// The combo box's or menu button's path, as `ui show` lists it.
        path: String,
        /// The option, as the list shows it.
        option: String,
    },
    /// Scroll a window's list to read what is below the fold. A control
    /// out of view needs none: working it scrolls it into view first.
    Scroll {
        /// The window, by its title.
        window: String,
        /// How far, in points: positive goes down, negative up.
        #[arg(allow_hyphen_values = true)]
        points: f32,
    },
}

#[derive(Args, Debug)]
pub struct StashArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// What: a thing in the agent's own world, by its name, or a catalogue
    /// entry, by its slug.
    pub what: String,
}

#[derive(Args, Debug)]
pub struct ItemArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// The item, by the name `inventory` gives it.
    pub item: String,
}

#[derive(Args, Debug)]
pub struct GiftArgs {
    #[command(subcommand)]
    pub action: GiftAction,
}

#[derive(Subcommand, Debug)]
pub enum GiftAction {
    /// Offer a player in the agent's world an inventory item, by its name,
    /// or a catalogue entry, by its slug. A gift is a copy: the agent keeps
    /// its own.
    Give(GiveArgs),
    /// Accept the admin's gift offer. Saved at once when the agent may save.
    Accept(OfferArgs),
    /// Decline the admin's gift offer.
    Decline(OfferArgs),
}

#[derive(Args, Debug)]
pub struct GiveArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Whom: a DID, or a handle (with or without its @).
    pub player: String,
    /// What: an inventory item's name, or a catalogue entry's slug.
    pub item: String,
    /// Wait for their answer - or for the offer to lapse - and print it.
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args, Debug)]
pub struct OfferArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// The offer, by the id its `gift_offered` event gives.
    pub offer_id: u64,
}

#[derive(Args, Debug)]
pub struct PlacementsArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Only those within this many metres of the agent.
    #[arg(long, value_name = "METRES")]
    pub within: Option<f32>,
}

#[derive(Args, Debug)]
pub struct CatalogueArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Words to look for in an entry's slug, name, section or description.
    pub search: Option<String>,
}

#[derive(Args, Debug)]
pub struct PlaceArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// The catalogue entry, by its slug (as `catalogue` lists them).
    pub slug: String,
    /// Where: a point's x and z in world metres. A few metres ahead of the
    /// agent when absent.
    #[arg(
        long,
        num_args = 2,
        value_names = ["X", "Z"],
        allow_negative_numbers = true
    )]
    pub at: Option<Vec<f32>>,
    /// Which way it faces, in degrees clockwise seen from above: 0 faces
    /// -Z, 90 faces +X.
    #[arg(long, value_name = "DEGREES", allow_negative_numbers = true)]
    pub yaw: Option<f32>,
}

#[derive(Args, Debug)]
pub struct MoveArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Which placement, by the index `placements` gives it.
    pub index: usize,
    /// The point's x, in world metres.
    #[arg(allow_negative_numbers = true)]
    pub x: f32,
    /// The point's z, in world metres.
    #[arg(allow_negative_numbers = true)]
    pub z: f32,
    /// Turn it to face this way too, in degrees clockwise seen from above:
    /// 0 faces -Z, 90 faces +X.
    #[arg(long, value_name = "DEGREES", allow_negative_numbers = true)]
    pub yaw: Option<f32>,
}

#[derive(Args, Debug)]
pub struct RemoveArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Which placement, by the index `placements` gives it. The ones after
    /// it move down one.
    pub index: usize,
}

#[derive(Args, Debug)]
pub struct RecordJsonArgs {
    #[command(subcommand)]
    pub action: JsonAction,
}

#[derive(Subcommand, Debug)]
pub enum JsonAction {
    /// Print the record as JSON - or the part at a JSON pointer.
    Get(JsonGetArgs),
    /// Replace the part at a JSON pointer with a JSON value. Numbers are
    /// written the way the record stores them: whole numbers, a decimal
    /// scaled by 10 000 (1.5 is 15000).
    Set(JsonSetArgs),
}

#[derive(Args, Debug)]
pub struct JsonGetArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// A JSON pointer, such as /environment or /placements/3; the whole
    /// record when absent.
    pub pointer: Option<String>,
}

#[derive(Args, Debug)]
pub struct JsonSetArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Where: a JSON pointer, such as /environment/fog_visibility; '' for
    /// the whole record.
    pub pointer: String,
    /// The new value, as JSON - a string needs its quotes: '"text"'.
    #[arg(required_unless_present = "file", conflicts_with = "file")]
    pub value: Option<String>,
    /// Read the new value from this file instead.
    #[arg(long, value_name = "PATH")]
    pub file: Option<std::path::PathBuf>,
}

#[derive(Args, Debug)]
pub struct RecordArg {
    #[command(flatten)]
    pub account: AccountArg,
    /// Which record: the agent's world, its avatar, or its inventory.
    #[arg(value_enum, default_value_t = EditRecord::Room)]
    pub record: EditRecord,
}

#[derive(Args, Debug)]
pub struct SaveArgs {
    #[command(flatten)]
    pub record: RecordArg,
    /// Wait for the save to land - saved or failed - and print how.
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args, Debug)]
pub struct FollowArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Whom: a DID, or a handle (with or without its @).
    pub peer: String,
    /// How close to keep, in metres.
    #[arg(long, value_name = "METRES", default_value_t = crate::config::agent::FOLLOW_DISTANCE_M)]
    pub distance: f32,
    /// Run all the way, not only to catch up.
    #[arg(long)]
    pub run: bool,
    /// Wait for the follow to end - halted, the player gone, or travel - and
    /// print how.
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args, Debug)]
pub struct FaceArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Whom or where: a player's DID or handle, or a point's x and z in
    /// world metres.
    #[arg(
        num_args = 1..=2,
        value_names = ["PLAYER_OR_X", "Z"],
        allow_negative_numbers = true,
        required = true
    )]
    pub target: Vec<String>,
    /// Wait for the turn to end and print how.
    #[arg(long)]
    pub wait: bool,
}

#[derive(Args, Debug)]
pub struct LookArgs {
    #[command(flatten)]
    pub account: AccountArg,
    /// Where the picture is taken from.
    #[arg(long, value_enum, default_value_t = LookView::Play)]
    pub view: LookView,
    /// Which way to look, in degrees clockwise from where the agent faces:
    /// 0 ahead, 90 right, 180 behind, -90 left.
    #[arg(
        long,
        value_name = "DEGREES",
        allow_negative_numbers = true,
        conflicts_with = "at"
    )]
    pub heading: Option<f32>,
    /// Look toward a point on the ground instead: its x and z, in world
    /// metres (as `status` gives positions).
    #[arg(
        long,
        num_args = 2,
        value_names = ["X", "Z"],
        allow_negative_numbers = true
    )]
    pub at: Option<Vec<f32>>,
    /// Write the picture here rather than in the agent's own directory,
    /// which keeps only the most recent ones.
    #[arg(long, value_name = "PATH")]
    pub out: Option<std::path::PathBuf>,
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
    /// Leave even though the agent's own world has unsaved edits, and lose
    /// them. Without this or --save-edits, such a trip is refused.
    #[arg(long, conflicts_with = "save_edits")]
    pub discard_edits: bool,
    /// Save the agent's own world's unsaved edits, and leave once the save
    /// has landed. Needs --allow-save at start.
    #[arg(long)]
    pub save_edits: bool,
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
    /// Offline only: stand in as this identity instead - its seeded world
    /// and its seeded body. A DID no directory knows keeps them seeded:
    /// `did:plc:agentofflinecar22222222e` drives a car,
    /// `did:plc:agentofflineboat2222222d` a hover-boat, and
    /// `did:plc:agentofflineair222222222` flies an airship. Other commands
    /// reach it with `--account <DID>`.
    #[arg(long, value_name = "DID", requires = "offline", value_parser = room_did)]
    pub stand_in: Option<String>,
    /// Testing only, offline: fly the default airplane in place of the
    /// stand-in's own locomotion, its body left as it is - no seeded body
    /// is an airplane (#1431).
    #[arg(long, requires = "offline", hide = true)]
    pub wear_airplane: bool,
    /// The one player whose chat the agent hears, as a handle or DID. Every
    /// other player's lines are dropped unread, so nobody else can talk the
    /// agent into anything; with no admin it hears no chat at all. A handle
    /// that does not resolve stops the start.
    #[arg(long, value_name = "HANDLE_OR_DID", value_parser = account_name)]
    pub admin: Option<String>,
    /// Let the agent save its world and its avatar to its account, where
    /// every visitor sees them. Without it the agent can still edit - what
    /// it changes is seen by whoever is there, and gone when it stops - but
    /// `save` is refused.
    #[arg(long)]
    pub allow_save: bool,
}

/// `run`: what `start` takes, and what `start` hands the daemon it launches.
#[derive(Args, Debug)]
pub struct DaemonArgs {
    #[command(flatten)]
    pub run: RunArgs,
    /// The handle `start` resolved `--admin` from, so `status` can show it
    /// without looking it up again. A label only: the admin is the DID.
    #[arg(long, hide = true, requires = "admin", value_name = "HANDLE")]
    pub admin_handle: Option<String>,
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

/// `--admin`: a DID, or a handle with or without its @. Handles are
/// lower-case ASCII names with a dot in them; anything else is a typo best
/// caught before a lookup is spent on it.
fn account_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.starts_with("did:") {
        return room_did(name);
    }
    let handle = name.trim_start_matches('@').to_ascii_lowercase();
    let well_formed = handle.contains('.')
        && !handle.starts_with(['.', '-'])
        && !handle.ends_with(['.', '-'])
        && handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if well_formed {
        Ok(handle)
    } else {
        Err(format!(
            "{name:?} is neither a handle (name.example.com) nor a DID (did:plc:...)"
        ))
    }
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

    /// `--admin` takes a handle the way a person writes one - with its @, in
    /// any case - or a DID, and refuses what is neither before anything is
    /// looked up.
    #[test]
    fn an_admin_is_a_handle_or_a_did() {
        let admin = |raw: &str| {
            let cli = Cli::try_parse_from(["agent", "start", "--admin", raw])?;
            let Command::Start(args) = cli.command else {
                panic!("start");
            };
            Ok::<_, clap::Error>(args.admin)
        };
        assert_eq!(
            admin("@Codewright.bsky.social").unwrap().as_deref(),
            Some("codewright.bsky.social")
        );
        assert_eq!(
            admin("did:plc:z5yhcebtrvzblrojezn6pjgi")
                .unwrap()
                .as_deref(),
            Some("did:plc:z5yhcebtrvzblrojezn6pjgi")
        );
        for typo in [
            "codewright",
            "code wright.bsky.social",
            "@",
            "did:plc:",
            "-a.test",
        ] {
            assert!(admin(typo).is_err(), "{typo:?} was taken");
        }
        let cli = Cli::try_parse_from(["agent", "start"]).expect("parses");
        let Command::Start(args) = cli.command else {
            panic!("start");
        };
        assert_eq!(args.admin, None, "no admin unless one is named");
    }

    /// The label `start` hands its daemon is `run`'s alone - an operator
    /// cannot put a name beside a DID on `start` - and means nothing
    /// without the DID it labels.
    #[test]
    fn only_run_takes_the_admins_handle_and_only_with_an_admin() {
        let run = Cli::try_parse_from([
            "agent",
            "run",
            "--admin",
            "did:plc:admin",
            "--admin-handle",
            "admin.test",
        ])
        .expect("parses");
        let Command::Run(args) = run.command else {
            panic!("run");
        };
        assert_eq!(args.run.admin.as_deref(), Some("did:plc:admin"));
        assert_eq!(args.admin_handle.as_deref(), Some("admin.test"));

        assert!(
            Cli::try_parse_from(["agent", "run", "--admin-handle", "admin.test"]).is_err(),
            "a label with no admin"
        );
        assert!(
            Cli::try_parse_from([
                "agent",
                "start",
                "--admin",
                "did:plc:admin",
                "--admin-handle",
                "admin.test",
            ])
            .is_err(),
            "start resolves its own"
        );
    }

    /// A look is from the game's camera, straight ahead, unless it says
    /// otherwise; a point to look at takes minus signs, and a heading and a
    /// point are two answers to one question.
    #[test]
    fn a_look_takes_a_view_and_one_direction() {
        let parse = |args: &[&str]| {
            let cli = Cli::try_parse_from(["agent", "look"].iter().chain(args))?;
            let Command::Look(look) = cli.command else {
                panic!("look");
            };
            Ok::<_, clap::Error>(look)
        };
        let plain = parse(&[]).expect("parses");
        assert_eq!(plain.view, LookView::Play);
        assert_eq!((plain.heading, plain.at.as_deref()), (None, None));

        let eyes = parse(&["--view", "eyes", "--heading", "-90"]).expect("parses");
        assert_eq!((eyes.view, eyes.heading), (LookView::Eyes, Some(-90.0)));

        let at = parse(&["--at", "-12.5", "40"]).expect("parses");
        assert_eq!(at.at.as_deref(), Some(&[-12.5, 40.0][..]));

        assert!(parse(&["--heading", "90", "--at", "1", "2"]).is_err());
        assert!(
            parse(&["--at", "1"]).is_err(),
            "a point has two coordinates"
        );
    }

    /// `face` takes one player or two coordinates, minus signs and all.
    #[test]
    fn a_face_takes_a_player_or_a_point() {
        let target = |args: &[&str]| {
            let cli = Cli::try_parse_from(["agent", "face"].iter().chain(args))?;
            let Command::Face(face) = cli.command else {
                panic!("face");
            };
            Ok::<_, clap::Error>(face.target)
        };
        assert_eq!(target(&["@friend.test"]).unwrap(), ["@friend.test"]);
        assert_eq!(target(&["-4.5", "12"]).unwrap(), ["-4.5", "12"]);
        assert!(target(&[]).is_err(), "whom or where");
        assert!(target(&["1", "2", "3"]).is_err());
    }

    /// The edit commands take points and turns with minus signs, a
    /// placement by its index, and a record - the world unless the avatar
    /// is named (#1422).
    #[test]
    fn edits_take_points_indices_and_a_record() {
        let parse = |args: &[&str]| {
            Cli::try_parse_from(std::iter::once("agent").chain(args.iter().copied()))
                .map(|cli| cli.command)
        };
        let Command::Place(place) =
            parse(&["place", "lamp_post", "--at", "-4.5", "12", "--yaw", "-90"]).unwrap()
        else {
            panic!("place");
        };
        assert_eq!(place.slug, "lamp_post");
        assert_eq!(place.at.as_deref(), Some(&[-4.5, 12.0][..]));
        assert_eq!(place.yaw, Some(-90.0));
        assert!(parse(&["place", "lamp_post", "--at", "1"]).is_err());

        let Command::Move(moved) = parse(&["move", "3", "-1", "-2.5"]).unwrap() else {
            panic!("move");
        };
        assert_eq!(
            (moved.index, moved.x, moved.z, moved.yaw),
            (3, -1.0, -2.5, None)
        );
        assert!(
            parse(&["remove", "-1"]).is_err(),
            "an index is never negative"
        );

        let Command::Undo(undo) = parse(&["undo"]).unwrap() else {
            panic!("undo");
        };
        assert_eq!(undo.record, EditRecord::Room);
        let Command::Save(save) = parse(&["save", "avatar", "--wait"]).unwrap() else {
            panic!("save");
        };
        assert_eq!(save.record.record, EditRecord::Avatar);
        assert!(save.wait);
        let Command::Revert(revert) = parse(&["revert", "inventory"]).unwrap() else {
            panic!("revert");
        };
        assert_eq!(revert.record, EditRecord::Inventory);
        assert!(parse(&["revert", "world"]).is_err());
    }

    /// The inventory's and the gifts' commands (#1423): an item by name, a
    /// player and an item for a gift, an offer by its number.
    #[test]
    fn inventory_and_gifts_take_names_players_and_offers() {
        let parse = |args: &[&str]| {
            Cli::try_parse_from(std::iter::once("agent").chain(args.iter().copied()))
                .map(|cli| cli.command)
        };
        let Command::Stash(stash) = parse(&["stash", "lighthouse"]).unwrap() else {
            panic!("stash");
        };
        assert_eq!(stash.what, "lighthouse");
        let Command::TakeOff(off) = parse(&["take-off", "Ship's Lantern"]).unwrap() else {
            panic!("take-off");
        };
        assert_eq!(off.item, "Ship's Lantern");
        assert!(parse(&["inventory"]).is_ok());
        assert!(parse(&["wear"]).is_err(), "wear what");

        let Command::Gift(gift) =
            parse(&["gift", "give", "@friend.test", "lantern", "--wait"]).unwrap()
        else {
            panic!("gift");
        };
        let GiftAction::Give(give) = gift.action else {
            panic!("give");
        };
        assert_eq!(
            (give.player.as_str(), give.item.as_str()),
            ("@friend.test", "lantern")
        );
        assert!(give.wait);
        let Command::Gift(gift) = parse(&["gift", "accept", "7"]).unwrap() else {
            panic!("gift");
        };
        assert!(matches!(gift.action, GiftAction::Accept(offer) if offer.offer_id == 7));
        assert!(parse(&["gift", "decline", "-1"]).is_err());
    }

    /// A JSON set takes its value inline or from a file - one of them.
    #[test]
    fn a_json_set_takes_a_value_or_a_file() {
        let parse = |args: &[&str]| {
            Cli::try_parse_from(std::iter::once("agent").chain(args.iter().copied()))
                .map(|cli| cli.command)
        };
        let Command::Room(room) = parse(&["room", "set", "/environment/fog", "5000"]).unwrap()
        else {
            panic!("room");
        };
        let JsonAction::Set(set) = room.action else {
            panic!("set");
        };
        assert_eq!(
            (set.pointer.as_str(), set.value.as_deref()),
            ("/environment/fog", Some("5000"))
        );
        assert!(parse(&["avatar", "set", "", "--file", "a.json"]).is_ok());
        assert!(parse(&["room", "set", "/x"]).is_err(), "no value");
        assert!(parse(&["room", "set", "/x", "1", "--file", "a.json"]).is_err());
        let Command::Avatar(avatar) = parse(&["avatar", "get"]).unwrap() else {
            panic!("avatar");
        };
        assert!(matches!(avatar.action, JsonAction::Get(get) if get.pointer.is_none()));
    }

    /// Unsaved edits are dropped or saved on a trip - not both - and saving
    /// at all is the operator's to allow at start.
    #[test]
    fn a_trip_drops_or_saves_edits_and_saving_is_allowed_at_start() {
        let travel = |extra: &[&str]| {
            Cli::try_parse_from(["agent", "travel", "home"].iter().chain(extra))
                .map(|cli| cli.command)
        };
        assert!(travel(&["--discard-edits"]).is_ok());
        assert!(travel(&["--save-edits"]).is_ok());
        assert!(travel(&["--discard-edits", "--save-edits"]).is_err());

        let Command::Start(start) = Cli::try_parse_from(["agent", "start", "--allow-save"])
            .unwrap()
            .command
        else {
            panic!("start");
        };
        assert!(start.allow_save);
        let Command::Start(start) = Cli::try_parse_from(["agent", "start"]).unwrap().command else {
            panic!("start");
        };
        assert!(!start.allow_save, "saving is off unless allowed");
    }

    /// Wearing the test airplane is an offline thing too.
    #[test]
    fn the_test_airplane_needs_offline() {
        assert!(Cli::try_parse_from(["agent", "start", "--offline", "--wear-airplane"]).is_ok());
        assert!(Cli::try_parse_from(["agent", "start", "--wear-airplane"]).is_err());
    }

    /// A stand-in is an offline thing; naming one online is a mistake.
    #[test]
    fn a_stand_in_needs_offline() {
        assert!(
            Cli::try_parse_from(["agent", "start", "--offline", "--stand-in", "did:plc:boat"])
                .is_ok()
        );
        assert!(Cli::try_parse_from(["agent", "start", "--stand-in", "did:plc:boat"]).is_err());
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

    /// `ui` alone asks what is open; its verbs take paths with spaces and
    /// `>`s as one argument, numbers with minus signs, and `--account`
    /// anywhere on the line (#1424).
    #[test]
    fn the_interface_takes_paths_numbers_and_an_account_anywhere() {
        let ui = |args: &[&str]| {
            let cli = Cli::try_parse_from(["agent", "ui"].iter().chain(args))?;
            let Command::Ui(ui) = cli.command else {
                panic!("ui");
            };
            Ok::<_, clap::Error>(ui)
        };
        assert!(ui(&[]).unwrap().action.is_none());
        assert!(ui(&["--picture"]).unwrap().picture);
        assert!(matches!(
            ui(&["show", "Avatar", "--picture"]).unwrap().action,
            Some(UiAction::Show { picture: true, .. })
        ));
        let typed = ui(&["type", "Avatar > Search", "lamp", "--enter"]).unwrap();
        assert!(matches!(
            typed.action,
            Some(UiAction::Type { ref path, ref text, enter: true })
                if path == "Avatar > Search" && text == "lamp"
        ));
        assert!(matches!(
            ui(&["set", "Avatar > hair > width", "-0.5"]).unwrap().action,
            Some(UiAction::Set { value, .. }) if value == -0.5
        ));
        assert!(matches!(
            ui(&["scroll", "Avatar", "-200"]).unwrap().action,
            Some(UiAction::Scroll { points, .. }) if points == -200.0
        ));
        let at_the_end = ui(&["show", "World Editor", "--account", "did:plc:a"]).unwrap();
        assert_eq!(at_the_end.account.as_deref(), Some("did:plc:a"));
        assert!(ui(&["click"]).is_err(), "a click names a control");
    }
}
