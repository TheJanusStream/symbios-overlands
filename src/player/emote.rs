//! Chat-keyword emotes: a message plays a gesture on its sender's body (#1068).
//!
//! The expressive half of the motion roster had no way into the world. Four
//! gestures — a greeting, a yes, a no and a bow — exist because
//! *"a social space is other people"* and they carry meaning to a viewer without
//! a shared language, and then nothing ever asked for one. This is the surface
//! that asks: say hello in chat and your avatar waves.
//!
//! **Chat text stays chat text.** There is no command syntax and no slash-emote
//! vocabulary to learn or to typo — the message is sent and displayed exactly as
//! written, and the gesture is a side effect of words the sender was going to
//! type anyway. That is deliberate: an emote nobody has to learn is one every
//! visitor uses on their first day.
//!
//! # The clip removal came through here, and this survived it verbatim
//!
//! This surface was built against the baked clips and designed to outlive
//! them, and it did (#1067): what this module owns is the *trigger* — the
//! keyword table, the arbitration, the timing — and none of that is clip
//! shaped. [`Emote::gesture_name`] now points at the engine's goal-space
//! gestures (symbios-avatar #248), which write only the parts they address,
//! so the old rule that a gesture rides the upper body while the legs go on
//! walking stopped being a rule this crate enforces and became a property of
//! the format. This module is also where each re-authored gesture is judged,
//! because it is the only place they are ever seen in motion.
//!
//! # What it does not do
//!
//! Only rigged bodies gesture. A generator chassis — a boat, an airship — has
//! no rig to pose and no gesture that would mean anything on it, so a keyword
//! from one is simply a chat message.

use bevy::prelude::*;

/// A gesture a chat message can ask for.
///
/// Deliberately a closed set of four, and the four are the ones the engine's
/// clip documentation argued a social space actually needs: a greeting, a
/// yes, a no and a bow. Adding a fifth means the engine authoring a gesture
/// for it, so the enum and the roster stay in step rather than drifting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Emote {
    /// A wave. Hello, goodbye, and the one every visitor tries first.
    Greeting,
    /// Yes.
    Nod,
    /// No.
    Reject,
    /// Thanks, respect, well played.
    Bow,
}

impl Emote {
    /// Every emote, for iteration and for the role indexer.
    pub const ALL: [Emote; 4] = [Emote::Greeting, Emote::Nod, Emote::Reject, Emote::Bow];

    /// The engine gesture this plays, by `gesture::by_name`'s own name.
    ///
    /// The names are the baked roster's, kept verbatim through the removal
    /// (#1067) because the engine's `gesture::by_name` answers to them by
    /// design (symbios-avatar #248). The pairing — every emote resolves a
    /// gesture — is guarded by test in `rigged`, so a rename on either side
    /// fails the suite instead of leaving a keyword silently inert.
    #[must_use]
    pub fn gesture_name(self) -> &'static str {
        match self {
            Emote::Greeting => "Greeting",
            Emote::Nod => "Head Nod",
            Emote::Reject => "Reject",
            Emote::Bow => "Bow",
        }
    }

    /// The words that ask for this emote.
    ///
    /// **Whole words, lowercase, matched against the message's own words.** A
    /// substring match would fire `Nod` on "another" and `Reject` on "nope
    /// worries"; more to the point it would make the feature unpredictable,
    /// and an emote that fires for reasons the sender cannot see is worse than
    /// no emote at all.
    ///
    /// Chosen for what people actually type in a room rather than for
    /// completeness. `o7` is a salute; `gg` and `ty` are the two most common
    /// thanks in any multiplayer text box.
    #[must_use]
    pub fn keywords(self) -> &'static [&'static str] {
        match self {
            Emote::Greeting => &[
                "hello",
                "hi",
                "hey",
                "heya",
                "yo",
                "hiya",
                "greetings",
                "bye",
                "goodbye",
                "cya",
                "o7",
                "wave",
            ],
            Emote::Nod => &[
                "yes", "yeah", "yep", "yup", "sure", "agreed", "nod", "ok", "okay",
            ],
            Emote::Reject => &["no", "nope", "nah", "never", "disagree"],
            Emote::Bow => &[
                "thanks", "thank", "thx", "ty", "gg", "bow", "respect", "please",
            ],
        }
    }

    /// A one-line hint naming one example word per emote — the chat
    /// window's caption and the Controls sheet's line (#1141).
    ///
    /// Built from [`Self::keywords`] rather than written out. The whole
    /// feature is invisible: there is no command syntax to discover and
    /// nothing in the UI ever said the words exist, so a first-session
    /// visitor could only find it by typing one by accident. A hint that
    /// named words the table no longer carries would be worse than none,
    /// so it reads the table.
    #[must_use]
    pub fn hint_line() -> String {
        let examples: Vec<&str> = Emote::ALL
            .iter()
            .filter_map(|emote| emote.keywords().first().copied())
            .collect();
        format!(
            "Say {} — and your avatar gestures as you type.",
            examples.join(", ")
        )
    }

    /// The emote a chat message asks for, if any.
    ///
    /// **First hit in the message wins, and the scan is by word position rather
    /// than by emote order.** "no thanks" is a refusal followed by a courtesy
    /// and reads as the refusal; scanning the emote list instead would let
    /// whichever variant happened to be declared first decide, which is an
    /// ordering nobody typing a message can see.
    ///
    /// Punctuation is stripped from each word so "hello!" and "yes," count. A
    /// word is compared lowercase, so shouting still waves.
    #[must_use]
    pub fn from_text(text: &str) -> Option<Emote> {
        text.split_whitespace().find_map(|word| {
            let word = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if word.is_empty() {
                return None;
            }
            Emote::ALL
                .into_iter()
                .find(|emote| emote.keywords().contains(&word.as_str()))
        })
    }
}

/// Ask a chassis's rigged body to play a gesture.
///
/// The target is the **chassis** — the physics entity a peer or the local
/// player is — rather than the rigged root under it, because that is the entity
/// the chat and network layers already hold. Resolving it to a body is
/// [`super::rigged::start_emotes`]'s job, and a chassis with no rigged body
/// simply drops the request.
#[derive(Message, Clone, Copy, Debug)]
pub struct EmoteRequest {
    /// The physics chassis whose body should gesture.
    pub chassis: Entity,
    /// What to play.
    pub emote: Emote,
}

/// Turn a chat message into an emote request, if it asks for one.
///
/// Shared by both trigger sites so the local echo and the remote path cannot
/// drift apart — the sender must see the same gesture everybody else sees, and
/// two copies of this rule is how that stops being true.
pub fn request_for(chassis: Entity, text: &str) -> Option<EmoteRequest> {
    Emote::from_text(text).map(|emote| EmoteRequest { chassis, emote })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_greeting_words_wave() {
        for text in ["hello", "hi there", "Hey!", "o7", "hello world"] {
            assert_eq!(
                Emote::from_text(text),
                Some(Emote::Greeting),
                "{text:?} should wave"
            );
        }
    }

    #[test]
    fn a_message_with_no_keyword_gestures_nothing() {
        for text in [
            "",
            "the weather is fine",
            "look at that mountain",
            "12345",
            "   ",
        ] {
            assert_eq!(Emote::from_text(text), None, "{text:?} should be silent");
        }
    }

    #[test]
    fn a_keyword_is_a_whole_word_and_not_a_substring() {
        // The defect this guards: "another" contains "no", "nothing" contains
        // "no", "hint" contains "hi". Substring matching would gesture on all
        // three and the feature would read as random.
        for text in [
            "another one",
            "nothing much",
            "just a hint",
            "history",
            "yesterday",
            "notable",
        ] {
            assert_eq!(
                Emote::from_text(text),
                None,
                "{text:?} must not match a substring"
            );
        }
    }

    #[test]
    fn punctuation_and_case_do_not_hide_a_keyword() {
        assert_eq!(Emote::from_text("HELLO!"), Some(Emote::Greeting));
        assert_eq!(Emote::from_text("yes,"), Some(Emote::Nod));
        assert_eq!(Emote::from_text("...thanks!"), Some(Emote::Bow));
        assert_eq!(Emote::from_text("(nope)"), Some(Emote::Reject));
    }

    #[test]
    fn the_first_word_in_the_message_decides_not_the_first_emote_declared() {
        // "no thanks" is a refusal with a courtesy after it. Scanning the emote
        // list rather than the sentence would answer Bow here purely because of
        // where Bow sits in `ALL`, which is an ordering the sender cannot see.
        assert_eq!(Emote::from_text("no thanks"), Some(Emote::Reject));
        assert_eq!(Emote::from_text("thanks, no"), Some(Emote::Bow));
    }

    #[test]
    fn every_emote_names_a_gesture_the_engine_can_build() {
        // An emote whose name the engine does not answer to is silent at
        // runtime and silent in review — the keyword scans, the request is
        // written, and the drive's `by_name` lookup quietly returns `None`
        // every frame. So the names are checked against the engine's own
        // resolver rather than against a doc, and a rename on EITHER side
        // fails here: this is the guard that replaced the shipped-archive
        // check when the archive stopped shipping (#1067).
        use symbios_avatar::anim::gesture;
        for emote in Emote::ALL {
            assert!(
                gesture::by_name(emote.gesture_name()).is_some(),
                "{emote:?} names {:?}, which gesture::by_name cannot build",
                emote.gesture_name()
            );
        }
    }

    #[test]
    fn no_two_emotes_claim_the_same_word() {
        // A word in two tables makes `from_text` depend on `ALL`'s order, which
        // is exactly the invisible ordering the sentence scan exists to avoid.
        for (index, emote) in Emote::ALL.into_iter().enumerate() {
            for other in Emote::ALL.into_iter().skip(index + 1) {
                for word in emote.keywords() {
                    assert!(
                        !other.keywords().contains(word),
                        "{word:?} is claimed by both {emote:?} and {other:?}"
                    );
                }
            }
        }
    }
}
