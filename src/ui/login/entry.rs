//! The landmark-link entry surface (#1227 f250/f294): naming the
//! destination a link chose, and the override warning that must be read
//! before it is accepted.
//!
//! A landmark link is the product's own sharing mechanism, so its recipient
//! is by definition somebody who has never seen the app. Until this module
//! existed the link's `did=` did two things at once: it filled in the
//! destination field, and it submitted the form on the first frame — which
//! on wasm is a full-page navigation to an OAuth consent screen. The
//! stranger's first experience of Symbios Overlands was a third party
//! asking for access to their Bluesky account, on behalf of an app they had
//! not been told the name of, to visit a world named only by a raw DID they
//! never read.
//!
//! The same query string can also carry `pds=` and `relay=`, and both are
//! used verbatim: the PDS becomes the authorization server the browser is
//! navigated to, and the relay carries every chat line, transform, identity
//! announce and gift envelope of the session. Those two fields were
//! rendered inside a collapsed "Advanced" fold, so a link that repointed a
//! stranger's infrastructure did it with no click and nothing on screen.
//!
//! What this module adds:
//!
//! * [`resolve_boot_destination`] — one background lookup that turns the
//!   link's DID into a *verified* handle (see
//!   [`crate::pds::resolve_did_handle`]), so the card can say "@alice"
//!   rather than "did:plc:z72i7hdy…".
//! * [`DestinationLabel`] — where that answer lives, and
//!   [`DestinationLabel::name`], which spells it through the app's one
//!   naming ladder ([`PeerLabel`]) rather than inventing a sixth.
//! * [`override_warning`] — the sentence shown above a force-opened
//!   Advanced fold when the link brought infrastructure with it.
//!
//! The decision itself — ask, submit, or do nothing — is
//! [`crate::boot_params::entry_plan`], beside the params it reads.

use bevy::prelude::*;

use crate::network::presence::PeerLabel;

/// How the link's destination should be named on the login card.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum LabelState {
    /// The lookup has not started, or is in flight.
    #[default]
    Resolving,
    /// A handle that resolved forward to this same DID.
    Named(String),
    /// No verified handle: an unresolvable DID method, a claim that did
    /// not verify, or a lookup that failed. The DID stands on its own.
    Unnamed,
}

/// The verified name for the boot destination, if it has one (#1227 f250).
///
/// A resource rather than a `Local` for the reason [`super::LoginError`]
/// gives: the system that resolves it and the system that draws it are
/// different systems, and a `Local` would give each its own copy.
#[derive(Resource, Default)]
pub struct DestinationLabel {
    /// The DID the answer is about. An empty string means "no lookup has
    /// been started", which is also how a destination the user edits by
    /// hand invalidates the previous answer.
    pub did: String,
    pub state: LabelState,
}

impl DestinationLabel {
    /// The name to print for `did`, through the app's one naming ladder.
    ///
    /// [`PeerLabel::addressed`] is deliberate and is the whole reason this
    /// returns a `String` rather than an `Option`: `@alice.bsky.social` for
    /// a verified handle, `did:plc:z72i7hdy…` for everything else, and the
    /// `@` sigil attached to the handle tier and nothing else (#1218 f299).
    /// The login card, the roster row, the chat author tag, the arrival
    /// line and the gift modal now all print the same shape.
    pub fn name(&self, did: &str) -> String {
        let handle = match (&self.state, self.did == did) {
            (LabelState::Named(handle), true) => Some(handle.as_str()),
            _ => None,
        };
        PeerLabel::new(handle, Some(did)).addressed()
    }

    /// Whether a lookup for `did` is still outstanding — the card says
    /// "checking who that is" rather than settling on the DID too early.
    pub fn is_resolving(&self, did: &str) -> bool {
        self.did != did || self.state == LabelState::Resolving
    }
}

/// In-flight DID → handle lookup for the boot destination.
#[derive(Component)]
pub struct ResolveDestinationTask {
    did: String,
    task: bevy::tasks::Task<Option<String>>,
}

/// Start (and finish) the one lookup that names the link's destination.
///
/// Runs in `AppState::Login`. Exactly one lookup per DID: the guard is
/// [`DestinationLabel::did`], which is set the moment the task is spawned,
/// so a failed lookup settles on [`LabelState::Unnamed`] and is not retried
/// in a loop against somebody else's directory service.
///
/// Skipped entirely for a destination with no `did:` prefix — the form also
/// accepts an `@handle`, which is already a name and needs no lookup — and
/// for a DID method the network cannot resolve.
pub fn resolve_boot_destination(
    mut commands: Commands,
    boot: Option<Res<crate::boot_params::BootParams>>,
    mut label: ResMut<DestinationLabel>,
    mut tasks: Query<(Entity, &mut ResolveDestinationTask)>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        if label.did != task.did {
            // The destination moved under the lookup; its answer is about
            // somebody else now.
            continue;
        }
        label.state = match result {
            Some(handle) => LabelState::Named(handle),
            None => LabelState::Unnamed,
        };
    }

    let Some(did) = boot.as_deref().and_then(|b| b.target_did.clone()) else {
        return;
    };
    if !did.starts_with("did:") || label.did == did {
        return;
    }
    if !crate::pds::xrpc::is_resolvable_did(&did) {
        label.did = did;
        label.state = LabelState::Unnamed;
        return;
    }
    label.did = did.clone();
    label.state = LabelState::Resolving;
    let lookup_did = did.clone();
    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            crate::pds::resolve_did_handle(&client, &lookup_did).await
        };
        crate::config::http::run_or(fut, None).await
    });
    commands.spawn(ResolveDestinationTask { did, task });
}

/// The warning shown above a force-opened Advanced fold when the link
/// brought infrastructure with it (#1227 f294), or `None`.
///
/// Pure, and it names the hosts rather than saying "some settings were
/// changed": the user's only defence against a hostile landmark link is
/// recognising the two strings, and a warning that withholds them asks
/// them to take the app's word for it.
pub fn override_warning(pds: Option<&str>, relay: Option<&str>) -> Option<String> {
    match (pds, relay) {
        (None, None) => None,
        (Some(pds), None) => Some(format!(
            "This link also changes where you sign in: {pds}. Your account \
             password and this app's access are handled by that server."
        )),
        (None, Some(relay)) => Some(format!(
            "This link also changes which server carries the room: {relay}. \
             Everything you say and do in the world passes through it."
        )),
        (Some(pds), Some(relay)) => Some(format!(
            "This link also changes where you sign in ({pds}) and which \
             server carries the room ({relay}). The first handles your \
             account; everything you say and do in the world passes through \
             the second."
        )),
    }
}

/// The lead line of the confirmation card: where this link goes.
pub fn destination_line(name: &str, resolving: bool) -> String {
    if resolving {
        format!("You're heading to {name} — checking who that is…")
    } else {
        format!("You're heading to {name}'s overland.")
    }
}

/// The primary button's label once a destination is named (#1227 f250).
///
/// The button used to say "Enter the Overlands" for every destination,
/// which is true and useless: the one thing a link recipient needs to
/// decide is whose world they are about to authorise into.
pub fn enter_button_label(name: Option<&str>) -> String {
    match name {
        Some(name) => format!("Enter {name}'s overland"),
        None => String::from("Enter the Overlands"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot_params::{BootParams, BootSource, EntryPlan, entry_plan};

    fn link(did: Option<&str>) -> BootParams {
        BootParams {
            target_did: did.map(str::to_owned),
            autosubmit: did.is_some(),
            source: BootSource::Link,
            ..BootParams::default()
        }
    }

    /// #1227 f250. The sequence: a stranger clicks a friend's landmark link,
    /// the page flashes, and they are on a bsky.social consent screen for an
    /// app they have never heard of, to visit a world named by a DID they
    /// never read. The link must now ask.
    #[test]
    fn a_link_asks_before_it_signs_you_in() {
        assert_eq!(
            entry_plan(&link(Some("did:plc:friend")), false, false),
            EntryPlan::Confirm
        );
    }

    /// ...but a `--did` typed at a shell on this machine is the person
    /// sitting here saying where to go, and asking them again is noise.
    #[test]
    fn a_destination_typed_at_the_shell_still_submits_itself() {
        let cli = BootParams {
            source: BootSource::Cli,
            ..link(Some("did:plc:friend"))
        };
        assert_eq!(entry_plan(&cli, false, false), EntryPlan::Auto);
    }

    /// #1227 f294. The sequence: the same query string carries `pds=` and
    /// `relay=`, they land in fields inside a collapsed fold, and the app
    /// signs in through the stranger's infrastructure with no click and no
    /// disclosure. Neither source may auto-submit under an override.
    #[test]
    fn an_infrastructure_override_always_costs_a_click() {
        for source in [BootSource::Link, BootSource::Cli] {
            let with_pds = BootParams {
                source,
                pds: Some("https://evil.example".into()),
                ..link(Some("did:plc:friend"))
            };
            assert_eq!(entry_plan(&with_pds, false, false), EntryPlan::Confirm);
            let with_relay = BootParams {
                source,
                relay: Some("relay.evil.example".into()),
                ..link(Some("did:plc:friend"))
            };
            assert_eq!(entry_plan(&with_relay, false, false), EntryPlan::Confirm);
        }
    }

    /// #1230 f19. The sequence: a link visitor's destination is unreachable,
    /// they hit "Back to login" on a loading screen that has been retrying
    /// for minutes (or Log out from the account chip) — and the form
    /// auto-submits the same broken destination the instant it renders, so
    /// killing the app is the only exit. Once spent, the link stops
    /// submitting itself from EITHER source.
    #[test]
    fn a_spent_link_never_resubmits_itself() {
        let cli = BootParams {
            source: BootSource::Cli,
            ..link(Some("did:plc:friend"))
        };
        assert_eq!(entry_plan(&cli, true, false), EntryPlan::Confirm);
        assert_eq!(
            entry_plan(&link(Some("did:plc:friend")), true, false),
            EntryPlan::Confirm
        );
    }

    /// ...and the pre-fill survives, which is the half #1230 f19 asked to
    /// keep: `Confirm` still names the destination and fills the field, so a
    /// deliberate retry is one click.
    #[test]
    fn a_spent_link_is_still_one_click_from_a_retry() {
        assert_ne!(
            entry_plan(&link(Some("did:plc:friend")), true, false),
            EntryPlan::Idle,
            "the destination is still on the card"
        );
    }

    /// No destination is no decision, and a wasm persisted session owns the
    /// link itself (`check_wasm_resume` applies the `did=` override), so
    /// submitting on top would spawn two competing auth tasks.
    #[test]
    fn nothing_to_decide_is_an_idle_form() {
        assert_eq!(entry_plan(&link(None), false, false), EntryPlan::Idle);
        assert_eq!(
            entry_plan(&link(Some("did:plc:friend")), false, true),
            EntryPlan::Idle
        );
    }

    /// The name on the card is the app's one ladder, not a sixth spelling:
    /// `@handle` only for a verified handle, the elided DID otherwise, and
    /// never an `@` in front of an identifier (#1218 f299).
    #[test]
    fn the_card_names_the_destination_the_way_every_other_surface_does() {
        let mut label = DestinationLabel::default();
        let did = "did:plc:z72i7hdynmk6";
        assert_eq!(label.name(did), "did:plc:z72i7hdy…");
        assert!(label.is_resolving(did));

        label.did = did.to_owned();
        label.state = LabelState::Named("alice.bsky.social".into());
        assert_eq!(label.name(did), "@alice.bsky.social");
        assert!(!label.is_resolving(did));

        // An answer about a DIFFERENT destination must not be borrowed.
        assert_eq!(label.name("did:plc:someoneelse"), "did:plc:someonee…");
        assert!(label.is_resolving("did:plc:someoneelse"));

        label.state = LabelState::Unnamed;
        assert_eq!(label.name(did), "did:plc:z72i7hdy…");
        assert!(
            !label.is_resolving(did),
            "an unverifiable DID settles rather than spinning forever"
        );
    }

    /// #1227 f294's user-facing half: the warning names both hosts, because
    /// recognising the strings is the user's only defence.
    #[test]
    fn the_override_warning_names_the_hosts_it_is_warning_about() {
        assert_eq!(override_warning(None, None), None);
        let both = override_warning(Some("https://evil.example"), Some("relay.evil.example"))
            .expect("both");
        assert!(both.contains("https://evil.example"), "{both}");
        assert!(both.contains("relay.evil.example"), "{both}");
        let pds_only = override_warning(Some("https://evil.example"), None).expect("pds");
        assert!(pds_only.contains("https://evil.example"));
        assert!(!pds_only.contains("relay"));
        let relay_only = override_warning(None, Some("relay.evil.example")).expect("relay");
        assert!(relay_only.contains("relay.evil.example"));
    }

    /// The button says whose world it enters — the one thing a link
    /// recipient needs in order to decide.
    #[test]
    fn the_button_says_where_it_goes() {
        assert_eq!(enter_button_label(None), "Enter the Overlands");
        assert_eq!(
            enter_button_label(Some("@alice.bsky.social")),
            "Enter @alice.bsky.social's overland"
        );
        assert!(destination_line("@alice.bsky.social", false).contains("@alice.bsky.social"));
        assert!(destination_line("did:plc:z72i7hd…", true).contains("checking who that is"));
    }
}
