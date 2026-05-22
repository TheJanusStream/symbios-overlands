//! Teleporter pre-targeted at the local user's DID. Built for the
//! "hand a friend a way back to my room" flow: drag this entry onto a
//! peer row in the People window, and the resulting [`ItemOffer`]
//! carries a portal whose `target_did` is already filled with the
//! sender's DID. When the recipient places it, stepping through
//! lands them in the sender's room.
//!
//! This is the only catalogue entry that consumes `local_did` in
//! [`CatalogueEntry::build`] — every other entry ignores the
//! parameter and produces a pure blueprint. The arrival position is
//! the room origin `(0, 0, 0)` so the portal lands somewhere
//! predictable inside the sender's room without depending on a
//! seeded-defaults round-trip.
//!
//! [`ItemOffer`]: crate::network::OverlandsMessage

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{Fp3, Generator, GeneratorKind};

pub struct MyTeleporter;

impl CatalogueEntry for MyTeleporter {
    fn slug(&self) -> &'static str {
        "my_teleporter"
    }
    fn name(&self) -> &'static str {
        "My Teleporter"
    }
    fn description(&self) -> &'static str {
        "Portal that returns to your own room — gift it so friends can drop by."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Tools
    }
    fn build(&self, local_did: &str) -> Generator {
        Generator::from_kind(GeneratorKind::Portal {
            target_did: local_did.to_string(),
            target_pos: Fp3([0.0, 0.0, 0.0]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_stamps_local_did_into_portal() {
        let mut g = MyTeleporter.build("did:example:alice");
        sanitize_generator(&mut g);
        match &g.kind {
            GeneratorKind::Portal {
                target_did,
                target_pos,
            } => {
                assert_eq!(target_did, "did:example:alice");
                assert_eq!(*target_pos, Fp3([0.0, 0.0, 0.0]));
            }
            other => panic!("expected Portal, got {other:?}"),
        }
    }

    #[test]
    fn build_with_empty_did_yields_empty_target() {
        // The editor's "+ From Catalogue" submenu seeds the tree
        // without a DID; the empty string round-trips so the user can
        // fill it in by hand later without surprise placeholders.
        let g = MyTeleporter.build("");
        match &g.kind {
            GeneratorKind::Portal { target_did, .. } => assert_eq!(target_did, ""),
            other => panic!("expected Portal, got {other:?}"),
        }
    }
}
