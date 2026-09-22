//! Where the browser keeps saved sessions, and the rule that reads them
//! (#1408).
//!
//! One saved session PER ACCOUNT in `localStorage`, and a pointer in each
//! tab's `sessionStorage` naming the account that tab signed in as. That
//! pair is what lets two tabs of one browser hold two accounts: each tab
//! restores the account it signed in as, rather than whoever signed in last.
//!
//! Before this there was one slot for the whole browser
//! ([`super::PERSISTED_SESSION_KEY`]), so a second tab's sign-in overwrote
//! the first's, a reload came back as the wrong person - and then loaded
//! that person's settings - and one tab's token refresh rotated the refresh
//! token the other was still holding, expiring its session out from under
//! it.
//!
//! ## Why this is a trait and not `web_sys` calls
//!
//! Every line here is browser storage, which the test suite cannot reach:
//! there is no wasm runner, so a test behind `cfg(target_arch = "wasm32")`
//! is a test nothing runs (the note on [`super::SESSION_STORAGE_KEY`] says
//! the same about the keys). [`KeyValue`] is the whole browser surface this
//! needs - three methods - so the policy above it is target-neutral and the
//! tests below drive it through a map, two tabs at a time, sharing one
//! `localStorage` and holding a `sessionStorage` each, exactly as a browser
//! does.

use super::{LAST_SESSION_KEY, PERSISTED_SESSION_KEY, TAB_SESSION_KEY, account_session_key};

/// The three things this module does to a browser store. Implemented over
/// `web_sys::Storage` on wasm, and over a map in the tests below.
pub trait KeyValue {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
    fn remove(&self, key: &str);
}

/// What a page load should do with the browser's saved sessions (#1408).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionOnBoot {
    /// This tab is signed in as this account: restore it without asking,
    /// which is what a reload (or a restored tab) expects.
    Resume(String),
    /// This tab has signed in as nobody, but the browser has this account
    /// saved: OFFER it, as "Continue as @alice", and leave the form ready
    /// for somebody else (owner decision 2026-09-22).
    ///
    /// Deliberately not automatic. A second tab opened to run a second
    /// account would otherwise resume the first - building its world, and,
    /// if its access token had expired, rotating the refresh token the
    /// first tab is still holding, which expires that session under it.
    Offer(String),
    /// Nothing saved, or nothing that still parses.
    None,
}

/// Decide [`SessionOnBoot`] from the two pointers and what is saved:
/// `tab` is [`TAB_SESSION_KEY`]'s value, `last` is [`LAST_SESSION_KEY`]'s,
/// and `saved` answers whether an account's slot is there to restore.
///
/// A pointer naming an account with no slot is ignored rather than obeyed,
/// so a tab whose account was signed out in another tab falls back to what
/// a fresh tab would be offered.
pub fn session_on_boot(
    tab: Option<&str>,
    last: Option<&str>,
    saved: impl Fn(&str) -> bool,
) -> SessionOnBoot {
    if let Some(did) = tab.filter(|did| saved(did)) {
        return SessionOnBoot::Resume(did.to_owned());
    }
    if let Some(did) = last.filter(|did| saved(did)) {
        return SessionOnBoot::Offer(did.to_owned());
    }
    SessionOnBoot::None
}

/// One tab's view of the browser's session storage: its own
/// `sessionStorage` and the `localStorage` every tab shares.
pub struct SessionStore<'a> {
    /// `sessionStorage`: this tab's alone, kept across reloads and the
    /// OAuth redirect (a navigation within the same tab).
    pub tab: &'a dyn KeyValue,
    /// `localStorage`: shared by every tab of this origin.
    pub shared: &'a dyn KeyValue,
}

impl SessionStore<'_> {
    /// The account this tab signed in as.
    pub fn tab_did(&self) -> Option<String> {
        self.tab.get(TAB_SESSION_KEY).filter(|did| !did.is_empty())
    }

    /// The account a tab that signed in as nobody is offered.
    pub fn last_did(&self) -> Option<String> {
        self.shared
            .get(LAST_SESSION_KEY)
            .filter(|did| !did.is_empty())
    }

    /// One account's saved session, as stored.
    pub fn slot(&self, did: &str) -> Option<String> {
        self.shared.get(&account_session_key(did))
    }

    /// Store one account's saved session.
    pub fn write_slot(&self, did: &str, blob: &str) {
        self.shared.set(&account_session_key(did), blob);
    }

    /// Drop a blob that no longer parses, so the next load does not fail
    /// the same way.
    pub fn drop_unreadable_slot(&self, did: &str) {
        self.shared.remove(&account_session_key(did));
    }

    /// Make `did` this tab's account, and the browser's most recent.
    ///
    /// Always both: a tab that takes an account on is by definition the
    /// last to have used it, and the pair is what [`Self::on_boot`] reads.
    pub fn claim(&self, did: &str) {
        self.tab.set(TAB_SESSION_KEY, did);
        self.shared.set(LAST_SESSION_KEY, did);
    }

    /// Forget one account's saved session, wherever it is pointed at from.
    ///
    /// The tab pointer is left alone unless it names this account: another
    /// tab's pointer is not ours to clear, and it is per-tab storage we
    /// could not reach anyway.
    pub fn forget(&self, did: &str) {
        self.shared.remove(&account_session_key(did));
        if self.last_did().as_deref() == Some(did) {
            self.shared.remove(LAST_SESSION_KEY);
        }
        if self.tab_did().as_deref() == Some(did) {
            self.tab.remove(TAB_SESSION_KEY);
        }
    }

    /// Forget THIS TAB's saved session - logout, and a refresh the server
    /// refused. Another tab signed in as somebody else keeps its own.
    pub fn forget_tab(&self) {
        if let Some(did) = self.tab_did() {
            self.forget(&did);
        }
        self.tab.remove(TAB_SESSION_KEY);
    }

    /// Fold a pre-#1408 blob from the one shared slot into its own
    /// account's, once, so an upgrade does not sign anybody out.
    ///
    /// The legacy key goes whatever happens: nothing writes it again, and a
    /// blob that no longer parses would otherwise be re-read on every load.
    /// An account that already has a slot keeps it - that one was written
    /// by this build. The DID is read out of the JSON rather than through
    /// the blob type, which lives on the wasm side of the build.
    ///
    /// The migrated account becomes the browser's most recent but NOT this
    /// tab's: whoever was signed in before the upgrade is offered, not
    /// restored behind a click nobody has made.
    pub fn migrate_legacy(&self) {
        let Some(raw) = self.shared.get(PERSISTED_SESSION_KEY) else {
            return;
        };
        self.shared.remove(PERSISTED_SESSION_KEY);
        let Some(did) = serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|blob| blob.get("did")?.as_str().map(str::to_owned))
            .filter(|did| !did.is_empty())
        else {
            return;
        };
        if self.slot(&did).is_none() {
            self.write_slot(&did, &raw);
        }
        if self.last_did().is_none() {
            self.shared.set(LAST_SESSION_KEY, &did);
        }
    }

    /// What this page load should do, migrating a pre-#1408 blob on the way
    /// past.
    pub fn on_boot(&self) -> SessionOnBoot {
        self.migrate_legacy();
        session_on_boot(
            self.tab_did().as_deref(),
            self.last_did().as_deref(),
            |did| self.slot(did).is_some(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// A browser store. `localStorage` is one of these shared by every tab;
    /// `sessionStorage` is one per tab.
    #[derive(Default)]
    struct Map(RefCell<BTreeMap<String, String>>);

    impl KeyValue for Map {
        fn get(&self, key: &str) -> Option<String> {
            self.0.borrow().get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) {
            self.0.borrow_mut().insert(key.to_owned(), value.to_owned());
        }
        fn remove(&self, key: &str) {
            self.0.borrow_mut().remove(key);
        }
    }

    const ALICE: &str = "did:plc:alice";
    const BOB: &str = "did:plc:bob";

    /// A saved session, as far as this module cares: JSON with a `did`.
    fn blob(did: &str) -> String {
        format!(r#"{{"did":"{did}","handle":"{did}.test","token_set":"…"}}"#)
    }

    /// Sign `did` in from this tab, the way a completed login does.
    fn sign_in(store: &SessionStore<'_>, did: &str) {
        store.write_slot(did, &blob(did));
        store.claim(did);
    }

    /// THE SEQUENCE (#1408, the owner's request): two tabs of one browser,
    /// two accounts. Alice signs in first, bob second, and then alice's tab
    /// is reloaded - which used to come back as bob, because the browser
    /// had one slot and bob's sign-in had overwritten it.
    #[test]
    fn two_tabs_hold_two_accounts_and_each_reload_restores_its_own() {
        let shared = Map::default();
        let (tab_a, tab_b) = (Map::default(), Map::default());
        let a = SessionStore {
            tab: &tab_a,
            shared: &shared,
        };
        let b = SessionStore {
            tab: &tab_b,
            shared: &shared,
        };

        sign_in(&a, ALICE);
        sign_in(&b, BOB);

        assert_eq!(a.on_boot(), SessionOnBoot::Resume(ALICE.into()));
        assert_eq!(b.on_boot(), SessionOnBoot::Resume(BOB.into()));
        assert_eq!(
            a.slot(ALICE).as_deref(),
            Some(blob(ALICE).as_str()),
            "bob's sign-in left alice's saved session alone"
        );
    }

    /// A third tab is OFFERED the most recent account rather than signed
    /// into it, so opening one to add an account does not resume - and
    /// refresh - an account another tab is holding.
    #[test]
    fn a_fresh_tab_is_offered_the_last_account() {
        let shared = Map::default();
        let (tab_a, tab_c) = (Map::default(), Map::default());
        let a = SessionStore {
            tab: &tab_a,
            shared: &shared,
        };
        let c = SessionStore {
            tab: &tab_c,
            shared: &shared,
        };
        sign_in(&a, ALICE);

        assert_eq!(c.on_boot(), SessionOnBoot::Offer(ALICE.into()));
        // Taking the offer is what makes it this tab's, and only then does
        // a reload restore it.
        c.claim(ALICE);
        assert_eq!(c.on_boot(), SessionOnBoot::Resume(ALICE.into()));
        // Turning it down instead forgets the saved session for everybody,
        // which is the point of the button: on a shared computer it is the
        // only way to stop being offered somebody else's account.
        let tab_d = Map::default();
        let d = SessionStore {
            tab: &tab_d,
            shared: &shared,
        };
        d.forget(ALICE);
        assert_eq!(d.on_boot(), SessionOnBoot::None);
        assert_eq!(a.slot(ALICE), None);
    }

    /// Logging out in one tab takes that account's saved session with it
    /// and leaves the other tab's alone - the mirror of #1223 f292 one
    /// layer down.
    #[test]
    fn logging_out_in_one_tab_leaves_the_other_signed_in() {
        let shared = Map::default();
        let (tab_a, tab_b) = (Map::default(), Map::default());
        let a = SessionStore {
            tab: &tab_a,
            shared: &shared,
        };
        let b = SessionStore {
            tab: &tab_b,
            shared: &shared,
        };
        sign_in(&a, ALICE);
        sign_in(&b, BOB);

        a.forget_tab();

        assert_eq!(a.on_boot(), SessionOnBoot::Offer(BOB.into()));
        assert_eq!(
            b.on_boot(),
            SessionOnBoot::Resume(BOB.into()),
            "bob's tab is untouched by alice logging out"
        );
        assert_eq!(a.slot(ALICE), None, "and alice's saved session is gone");
    }

    /// The upgrade (#1408): a browser holding a pre-#1408 blob in the one
    /// shared slot. Nobody is signed out; the account becomes the one a tab
    /// is offered, the legacy key is consumed, and the second tab to look
    /// does not find it again.
    #[test]
    fn a_pre_1408_blob_becomes_its_own_accounts_slot_once() {
        let shared = Map::default();
        let (tab_a, tab_b) = (Map::default(), Map::default());
        shared.set(PERSISTED_SESSION_KEY, &blob(ALICE));
        let a = SessionStore {
            tab: &tab_a,
            shared: &shared,
        };
        let b = SessionStore {
            tab: &tab_b,
            shared: &shared,
        };

        assert_eq!(a.on_boot(), SessionOnBoot::Offer(ALICE.into()));
        assert_eq!(a.slot(ALICE).as_deref(), Some(blob(ALICE).as_str()));
        assert_eq!(
            shared.get(PERSISTED_SESSION_KEY),
            None,
            "the legacy slot is consumed, not left to be read again"
        );
        assert_eq!(b.on_boot(), SessionOnBoot::Offer(ALICE.into()));

        // A blob that no longer parses is dropped rather than re-read
        // forever, and offers nothing.
        let shared = Map::default();
        let tab = Map::default();
        shared.set(PERSISTED_SESSION_KEY, "{not json");
        let store = SessionStore {
            tab: &tab,
            shared: &shared,
        };
        assert_eq!(store.on_boot(), SessionOnBoot::None);
        assert_eq!(shared.get(PERSISTED_SESSION_KEY), None);
    }

    /// A migration must never overwrite a slot this build wrote: the legacy
    /// key can only hold a session at least as old as the upgrade.
    #[test]
    fn the_legacy_blob_never_overwrites_a_newer_slot() {
        let shared = Map::default();
        let tab = Map::default();
        let store = SessionStore {
            tab: &tab,
            shared: &shared,
        };
        sign_in(&store, ALICE);
        store.write_slot(ALICE, "fresh");
        shared.set(PERSISTED_SESSION_KEY, &blob(ALICE));

        assert_eq!(store.on_boot(), SessionOnBoot::Resume(ALICE.into()));
        assert_eq!(store.slot(ALICE).as_deref(), Some("fresh"));
    }

    /// Nothing writes the one shared slot any more (#1408).
    ///
    /// It is read exactly once, by [`SessionStore::migrate_legacy`], and
    /// removed on the way past. A `set` on it anywhere would put every tab
    /// back in one slot, which is the defect - and the browser side cannot
    /// do it by accident, because it no longer names the key at all.
    #[test]
    fn the_pre_1408_shared_slot_is_only_ever_read_and_removed() {
        let policy = include_str!("session_store.rs")
            .split_once("#[cfg(test)]")
            .expect("this file has tests")
            .0;
        assert!(
            !policy.contains(&format!("set({}", "PERSISTED_SESSION_KEY")),
            "the legacy slot must never be written again"
        );
        let browser = include_str!("wasm.rs");
        assert!(
            !browser.contains("PERSISTED_SESSION_KEY"),
            "the browser side goes through this module, which is what keeps \
             the legacy slot readable exactly once"
        );
    }

    /// A pointer naming an account whose slot is gone is ignored, and the
    /// tab falls back to what a fresh one would be offered.
    #[test]
    fn a_pointer_to_a_signed_out_account_is_ignored() {
        let saved = |did: &str| did == BOB;
        assert_eq!(
            session_on_boot(Some(ALICE), Some(BOB), saved),
            SessionOnBoot::Offer(BOB.into())
        );
        assert_eq!(
            session_on_boot(Some(ALICE), None, saved),
            SessionOnBoot::None
        );
        assert_eq!(session_on_boot(None, None, saved), SessionOnBoot::None);
    }
}
