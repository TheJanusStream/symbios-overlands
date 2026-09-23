//! The agent's admin (#1427): the one account whose chat the agent hears.
//!
//! Chat is the channel anyone in a room can reach the agent through, at
//! will and with any words they like; an agent that read all of it could be
//! prompt-injected by whoever walked up. So the agent listens to one
//! account, named when it starts, and every other player's lines are dropped
//! before their text is read (`daemon::observe::record_chat`). With no admin
//! it hears nobody at all.
//!
//! The admin is bound by DID, never by handle: a handle can be pointed at
//! another account, a DID cannot. `--admin` takes either, and a handle is
//! resolved once, before the daemon starts; a handle that does not resolve
//! stops the start rather than leave the agent running with chat on.
//!
//! Why a chat line's DID can be trusted: the game attributes each line to
//! the DID the relay's signed session map gives its sender, and drops chat
//! from a sender the map does not know (`network::inbound::chat`). The relay
//! binds a connection to a DID with a service-auth token the account's own
//! PDS signed, so a peer cannot speak as the admin over the data channel.
//! What is left to trust is the relay itself.

use bevy::prelude::*;
use serde_json::{Value, json};

/// The account the agent takes chat from. A resource of the daemon, and its
/// absence is the rule, not a gap: no admin, no chat.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Admin {
    pub did: String,
    /// The handle the operator named the admin by, if they used one. Shown,
    /// and never used to decide who the admin is.
    pub handle: Option<String>,
}

impl Admin {
    /// The admin `name` stands for - a DID as given, or a handle looked up -
    /// for an agent that plays as `agent_did`.
    pub fn resolve(name: &str, agent_did: &str) -> Result<Self, String> {
        let (did, handle) =
            super::resolve_name(name).map_err(|e| format!("the admin {name}: {e}"))?;
        if did == agent_did {
            return Err(
                "the admin must be another account: the agent never hears its own lines".to_owned(),
            );
        }
        Ok(Self { did, handle })
    }

    /// How `status` and `start` show the admin.
    pub fn to_json(&self) -> Value {
        json!({ "did": self.did, "handle": self.handle })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DID is taken as it is - nothing is looked up, so this runs offline.
    #[test]
    fn a_did_is_the_admin_as_given() {
        let admin = Admin::resolve(" did:plc:admin ", "did:plc:agent").expect("an admin");
        assert_eq!(
            admin,
            Admin {
                did: "did:plc:admin".into(),
                handle: None
            }
        );
    }

    /// An agent told to take chat from itself would hear nobody while
    /// `status` claimed otherwise; the start is refused instead.
    #[test]
    fn the_agent_cannot_be_its_own_admin() {
        let refused = Admin::resolve("did:plc:agent", "did:plc:agent").expect_err("refused");
        assert!(refused.contains("another account"), "{refused}");
    }
}
