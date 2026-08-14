//! Chat-keyword emotes: a message plays a gesture on its sender's body (#1068).
//!
//! The expressive half of the baked clip set had no way into the world. Four of
//! the twelve clips — a greeting, a yes, a no and a bow — were imported because
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
//! # Where this sits in the clip removal
//!
//! It plays the **baked** clips, which symbios-avatar's epic #237 is in the
//! middle of retiring — and that is not a contradiction. What this module owns is the *trigger*:
//! the keyword table, the arbitration, the timing, and the rule that a gesture
//! rides the upper body while the legs go on walking. None of that is clip
//! shaped. When symbios-avatar #248 re-authors the expressive set as goal-space
//! clips, [`Emote::clip_name`] points at the new ones and everything else here
//! is unchanged — and this module becomes the place each re-authored gesture is
//! judged, because it is the only place they are ever seen in motion.
//!
//! # What it does not do
//!
//! Only rigged bodies gesture. A generator chassis — a boat, an airship — has
//! no rig to pose and no clip that would mean anything on it, so a keyword from
//! one is simply a chat message.

use bevy::prelude::*;

/// A gesture a chat message can ask for.
///
/// Deliberately a closed set of four, and the four are the ones
/// `docs/clips.md` in the engine argued a social space actually needs: a
/// greeting, a yes, a no and a bow. Adding a fifth means adding a clip, so the
/// enum and the artifact stay in step rather than drifting.
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

    /// The baked clip this plays, by the artifact's own name.
    ///
    /// The names are `docs/clips.md`'s verbatim, exactly as the locomotion
    /// roles read them: a library missing one simply leaves that emote silent
    /// rather than substituting another.
    #[must_use]
    pub fn clip_name(self) -> &'static str {
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
    fn every_emote_names_a_clip_in_the_shipped_archive() {
        // The same guard the locomotion roles carry: an emote whose clip is not
        // in the artifact is silent at runtime and silent in review, so the
        // names are checked against the bytes rather than against a doc.
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/avatar.clips"))
            .expect("the shipped clip archive is readable");
        let library = symbios_avatar::ClipLibrary::read(&bytes).expect("the archive parses");
        for emote in Emote::ALL {
            assert!(
                library
                    .clips
                    .iter()
                    .any(|clip| clip.name == emote.clip_name()),
                "{emote:?} names {:?}, which the archive does not carry",
                emote.clip_name()
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
