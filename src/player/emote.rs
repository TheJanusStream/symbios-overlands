//! Chat-keyword emotes: a message plays a gesture on its sender's body (#1068).
//!
//! The expressive half of the motion roster had no way into the world. Four
//! gestures - a greeting, a yes, a no and a bow - exist because
//! *"a social space is other people"* and they carry meaning to a viewer without
//! a shared language, and then nothing ever asked for one. This is the surface
//! that asks: say hello in chat and your avatar waves.
//!
//! **Chat text stays chat text.** There is no command syntax and no slash-emote
//! vocabulary to learn or to typo - the message is sent and displayed exactly as
//! written, and the gesture is a side effect of words the sender was going to
//! type anyway. That is deliberate: an emote nobody has to learn is one every
//! visitor uses on their first day.
//!
//! # The clip removal came through here, and this survived it verbatim
//!
//! This surface was built against the baked clips and designed to outlive
//! them, and it did (#1067): what this module owns is the *trigger* - the
//! keyword table, the arbitration, the timing - and none of that is clip
//! shaped. [`Emote::gesture_name`] now points at the engine's goal-space
//! gestures (symbios-avatar #248), which write only the parts they address,
//! so the old rule that a gesture rides the upper body while the legs go on
//! walking stopped being a rule this crate enforces and became a property of
//! the format. This module is also where each re-authored gesture is judged,
//! because it is the only place they are ever seen in motion.
//!
//! # A body answers for itself
//!
//! A message is read three times, as a person would hear it, as a beast would
//! ([`BeastEmote`], #1806) and as a winged body would ([`WingedEmote`],
//! #1878), and the request carries all three, because the chat and network
//! layers that raise it know a chassis and not what stands on it.
//! [`super::rigged::start_emotes`] plays the one the body knows.
//!
//! # What it does not do
//!
//! Only rigged bodies gesture. A generator chassis - a boat, an airship - has
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
    /// design (symbios-avatar #248). The pairing - every emote resolves a
    /// gesture - is guarded by test in `rigged`, so a rename on either side
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

    /// A one-line hint naming one example word per emote - the chat
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
        hint_naming(&examples)
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

/// A gesture a chat message can ask of a beast (#1806).
///
/// **A beast answers a person's words with what it has.** A greeting, a yes and
/// a thanks are the words in [`Emote::keywords`], so nobody learns a second
/// vocabulary and a body switched mid-conversation still answers them. A
/// refusal has nothing to play on a beast, and still decides its sentence
/// ([`Self::from_text`]). Three more are a beast's alone: sit, lie down and get
/// up, which a person's table does not hear.
///
/// **This table is what keeps a person's gestures off a beast**: the engine
/// falls through to a person's gesture when a beast is asked for one by name
/// ([`symbios_avatar::anim::gesture::for_body`]), so a name from [`Emote`] must
/// never reach it. Guarded by test against the engine's own beast roster.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BeastEmote {
    /// A wagging tail, for a greeting.
    Wag,
    /// A tilted head, for a yes.
    HeadTilt,
    /// A play bow, for a thanks.
    PlayBow,
    /// Sits, and stays sitting until it moves or is told otherwise.
    Sit,
    /// Lies down, and stays down until it moves or is told otherwise.
    LieDown,
    /// Gets up where it is out of a sit or a lying down.
    GetUp,
}

impl BeastEmote {
    /// Every emote a beast has, in the order the hint names them.
    pub const ALL: [BeastEmote; 6] = [
        BeastEmote::Wag,
        BeastEmote::HeadTilt,
        BeastEmote::PlayBow,
        BeastEmote::Sit,
        BeastEmote::LieDown,
        BeastEmote::GetUp,
    ];

    /// The engine gesture this plays, by the name it answers to on a beast.
    #[must_use]
    pub fn gesture_name(self) -> &'static str {
        match self {
            BeastEmote::Wag => "Wag",
            BeastEmote::HeadTilt => "Head Tilt",
            BeastEmote::PlayBow => "Play Bow",
            BeastEmote::Sit => "Sit",
            BeastEmote::LieDown => "Lie Down",
            BeastEmote::GetUp => symbios_avatar::anim::emote::GET_UP,
        }
    }

    /// The words that ask for this emote, as [`Emote::keywords`] reads them.
    /// A keyword with a space in it is that many words in a row: "lie" on its
    /// own is an accusation and "down" on its own is a mood.
    #[must_use]
    pub fn keywords(self) -> &'static [&'static str] {
        match self {
            BeastEmote::Wag => Emote::Greeting.keywords(),
            BeastEmote::HeadTilt => Emote::Nod.keywords(),
            BeastEmote::PlayBow => Emote::Bow.keywords(),
            BeastEmote::Sit => &["sit"],
            BeastEmote::LieDown => &["lie down", "lay down"],
            BeastEmote::GetUp => &["get up", "up"],
        }
    }

    /// [`Emote::hint_line`] for a beast.
    #[must_use]
    pub fn hint_line() -> String {
        let examples: Vec<&str> = BeastEmote::ALL
            .iter()
            .filter_map(|emote| emote.keywords().first().copied())
            .collect();
        hint_naming(&examples)
    }

    /// The emote a chat message asks of a beast, if any.
    ///
    /// The rule is [`Emote::from_text`]'s: the first hit in the message wins.
    /// **A refusal is a hit with nothing to play**, so "no thanks" is no play
    /// bow: it reads as the refusal on a beast as it does on a person.
    #[must_use]
    pub fn from_text(text: &str) -> Option<BeastEmote> {
        first_hit(text, &BeastEmote::ALL, |emote| {
            emote.keywords().iter().copied()
        })
    }
}

/// The first of `all` a chat message asks for, by [`Emote::from_text`]'s rule:
/// the first hit in the message wins, a keyword with a space in it is that
/// many words in a row, and **a refusal is a hit with nothing to play**, so
/// "no thanks" plays nothing on a body that has nothing for "no".
fn first_hit<E: Copy, K: IntoIterator<Item = &'static str>>(
    text: &str,
    all: &[E],
    keywords: impl Fn(E) -> K,
) -> Option<E> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let says = |at: usize, keyword: &str| match keyword.split_once(' ') {
        Some((first, second)) => {
            words[at] == first && words.get(at + 1).is_some_and(|next| next == second)
        }
        None => words[at] == keyword,
    };
    (0..words.len()).find_map(|at| {
        let hit = all
            .iter()
            .copied()
            .find(|&emote| keywords(emote).into_iter().any(|keyword| says(at, keyword)));
        let refused = Emote::Reject.keywords().iter().any(|word| says(at, word));
        (hit.is_some() || refused).then_some(hit)
    })?
}

/// A gesture a chat message can ask of a winged body (#1878, the owner's
/// words): a bird's, a bat's or a sugar glider's, each played by the name its
/// own family's roster gives it ([`Self::gesture_name`]).
///
/// **The words are a person's and a beast's where they have one**, as a
/// beast's are a person's: a greeting plays a wing stretch, a yes a head tilt,
/// a thanks a preen, "sit" the family's rest and "lie down" a bird's roost or
/// a bat's or a glider's own rest, which is lying already, so a body switched
/// mid-conversation still answers them. A refusal plays
/// nothing and still decides its sentence. **And words of their own**: perch,
/// roost, rest, curl, preen, groom, stretch, shake and get up, where the three
/// rests' names, preen and groom are each one gesture's words on every family:
/// "perch" said to a bat rests it as a bat rests.
///
/// **This table keeps every other body's gestures off a winged one**, and the
/// engine would not have played them anyway: on a winged rig
/// [`symbios_avatar::anim::gesture::for_body`] answers only its own family's
/// roster ([`symbios_avatar::anim::winged::rests`]), so a name off a person's
/// or a beast's table plays nothing there. Guarded by test against the
/// engine's three rosters, a cross by its leading family's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WingedEmote {
    /// A wing stretch, a glider's bow-stretch, for a greeting.
    WingStretch,
    /// A tilted head, for a yes.
    HeadTilt,
    /// A preen, a bat's or a glider's groom, for a thanks.
    Preen,
    /// The family's rest - a bird's perch, a bat's rest, a glider's curl - held
    /// until it moves or is told otherwise.
    Rest,
    /// A bird's roost, lower still with its head tucked, held as a rest is. A
    /// bat or a glider has none, and lies down in its own rest, which is a
    /// lying pose: a bat's rest, a glider's curl.
    Roost,
    /// A shake of the trunk and the wings.
    Shake,
    /// Gets up where it is out of a rest.
    GetUp,
}

impl WingedEmote {
    /// Every emote a winged body has, in the order the hint names them.
    pub const ALL: [WingedEmote; 7] = [
        WingedEmote::WingStretch,
        WingedEmote::HeadTilt,
        WingedEmote::Preen,
        WingedEmote::Rest,
        WingedEmote::Roost,
        WingedEmote::Shake,
        WingedEmote::GetUp,
    ];

    /// The engine gesture this plays on a body `family` leads, by the name
    /// that family's roster gives it, or `None` for one it has no gesture
    /// for. A roost on a bat or a glider is its own lying rest.
    #[must_use]
    pub fn gesture_name(self, family: symbios_avatar::WingedFamily) -> Option<&'static str> {
        use symbios_avatar::WingedFamily::{Bat, Bird, Glider};
        Some(match (self, family) {
            (WingedEmote::WingStretch, Glider) => "Stretch",
            (WingedEmote::WingStretch, Bird | Bat) => "Wing Stretch",
            (WingedEmote::HeadTilt, _) => "Head Tilt",
            (WingedEmote::Preen, Bird) => "Preen",
            (WingedEmote::Preen, Bat | Glider) => "Groom",
            (WingedEmote::Rest, Bird) => "Perch",
            (WingedEmote::Rest, Bat) => "Rest",
            (WingedEmote::Rest, Glider) => "Curl",
            (WingedEmote::Roost, Bird) => "Roost",
            (WingedEmote::Roost, Bat) => "Rest",
            (WingedEmote::Roost, Glider) => "Curl",
            (WingedEmote::Shake, _) => "Shake",
            (WingedEmote::GetUp, _) => symbios_avatar::anim::emote::GET_UP,
        })
    }

    /// The words that ask for this emote, as [`BeastEmote::keywords`] reads
    /// them: a person's or a beast's first, then a winged body's own.
    pub fn keywords(self) -> impl Iterator<Item = &'static str> {
        let (shared, own): (&'static [&'static str], &'static [&'static str]) = match self {
            WingedEmote::WingStretch => (Emote::Greeting.keywords(), &["stretch"]),
            WingedEmote::HeadTilt => (Emote::Nod.keywords(), &[]),
            WingedEmote::Preen => (Emote::Bow.keywords(), &["preen", "groom"]),
            WingedEmote::Rest => (BeastEmote::Sit.keywords(), &["perch", "rest", "curl"]),
            WingedEmote::Roost => (BeastEmote::LieDown.keywords(), &["roost"]),
            WingedEmote::Shake => (&[], &["shake"]),
            WingedEmote::GetUp => (BeastEmote::GetUp.keywords(), &[]),
        };
        shared.iter().chain(own).copied()
    }

    /// The word a hint names for this emote on a body `family` leads, or
    /// `None` for one that plays nothing there or plays what another word
    /// already names: a rest by the family's own name for it, a roost by its
    /// own on a bird alone - a bat's or a glider's is its rest - and the rest
    /// by their first word.
    fn example(self, family: symbios_avatar::WingedFamily) -> Option<&'static str> {
        use symbios_avatar::WingedFamily::{Bat, Bird, Glider};
        self.gesture_name(family)?;
        Some(match (self, family) {
            (WingedEmote::Rest, Bird) => "perch",
            (WingedEmote::Rest, Bat) => "rest",
            (WingedEmote::Rest, Glider) => "curl",
            (WingedEmote::Roost, Bird) => "roost",
            (WingedEmote::Roost, Bat | Glider) => return None,
            _ => self.keywords().next()?,
        })
    }

    /// [`Emote::hint_line`] for a body `family` leads: only words that play on
    /// it.
    #[must_use]
    pub fn hint_line(family: symbios_avatar::WingedFamily) -> String {
        let examples: Vec<&str> = WingedEmote::ALL
            .iter()
            .filter_map(|emote| emote.example(family))
            .collect();
        hint_naming(&examples)
    }

    /// The emote a chat message asks of a winged body, if any: the rule is
    /// [`BeastEmote::from_text`]'s.
    #[must_use]
    pub fn from_text(text: &str) -> Option<WingedEmote> {
        first_hit(text, &WingedEmote::ALL, WingedEmote::keywords)
    }
}

/// The chat window's caption and the Controls sheet's line, naming `examples`.
fn hint_naming(examples: &[&str]) -> String {
    format!(
        // #1269 f136: the gesture is raised inside the SUBMIT branch,
        // after the message is pushed and immediately before the
        // broadcast - never while typing. This line is the only place
        // in the app that documents keyword emotes at all, so a hint
        // that misdescribes the trigger sends a first-session user
        // looking for a bug.
        "Say {} - and your avatar gestures when you send it.",
        examples.join(", ")
    )
}

/// The hint for the body `record` stands on (#1806): a beast's words on four
/// legs, a winged body's own family's (#1878, [`WingedEmote::hint_line`], a
/// cross's leading family's), and a person's otherwise, which is the line
/// every chassis has always been shown.
#[must_use]
pub fn hint_for(record: Option<&crate::pds::AvatarRecord>) -> String {
    use crate::pds::avatar::RiggedKind;
    use crate::pds::avatar::locomotion::{beast_on_foot, winged_on_foot};
    let winged = record
        .filter(|record| winged_on_foot(record).is_some())
        .and_then(|record| record.body.rigged_ref()?.resolved.as_ref())
        .map(|resolved| RiggedKind::of_record(&resolved.body));
    if let Some(RiggedKind::Winged(family)) = winged {
        WingedEmote::hint_line(family)
    } else if record.and_then(beast_on_foot).is_some() {
        BeastEmote::hint_line()
    } else {
        Emote::hint_line()
    }
}

/// Ask a chassis's rigged body to play a gesture.
///
/// The target is the **chassis** - the physics entity a peer or the local
/// player is - rather than the rigged root under it, because that is the entity
/// the chat and network layers already hold. Resolving it to a body is
/// [`super::rigged::start_emotes`]'s job, and a chassis with no rigged body
/// simply drops the request.
#[derive(Message, Clone, Copy, Debug)]
pub struct EmoteRequest {
    /// The physics chassis whose body should gesture.
    pub chassis: Entity,
    /// What to play on a person, if the message asks them for anything.
    pub emote: Option<Emote>,
    /// What to play on a beast, if the message asks one for anything.
    pub beast: Option<BeastEmote>,
    /// What to play on a winged body, if the message asks one for anything
    /// (#1878).
    pub winged: Option<WingedEmote>,
}

/// Turn a chat message into an emote request, if it asks for one.
///
/// Shared by both trigger sites so the local echo and the remote path cannot
/// drift apart - the sender must see the same gesture everybody else sees, and
/// two copies of this rule is how that stops being true.
pub fn request_for(chassis: Entity, text: &str) -> Option<EmoteRequest> {
    let (emote, beast, winged) = (
        Emote::from_text(text),
        BeastEmote::from_text(text),
        WingedEmote::from_text(text),
    );
    (emote.is_some() || beast.is_some() || winged.is_some()).then_some(EmoteRequest {
        chassis,
        emote,
        beast,
        winged,
    })
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
        // runtime and silent in review - the keyword scans, the request is
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
    fn a_beast_answers_a_persons_words_and_three_of_its_own() {
        use BeastEmote::*;
        for (text, answer) in [
            ("hello", Some(Wag)),
            ("Hi there!", Some(Wag)),
            ("yes", Some(HeadTilt)),
            ("thanks", Some(PlayBow)),
            ("sit", Some(Sit)),
            ("Sit!", Some(Sit)),
            ("lie down", Some(LieDown)),
            ("Lie  down.", Some(LieDown)),
            ("lay down", Some(LieDown)),
            ("get up", Some(GetUp)),
            ("up", Some(GetUp)),
            ("sit, thanks", Some(Sit)),
            ("thanks, no", Some(PlayBow)),
            // Nothing to play for a refusal, and it decides its sentence.
            ("no", None),
            ("no thanks", None),
            // Whole words, and the whole of a phrase.
            ("lie", None),
            ("down", None),
            ("lie to me, down there", None),
            ("the weather is fine", None),
            ("sitting", None),
            ("setup", None),
            ("", None),
        ] {
            assert_eq!(BeastEmote::from_text(text), answer, "{text:?}");
        }
    }

    #[test]
    fn a_person_does_not_hear_a_beasts_words() {
        for text in ["sit", "lie down", "lay down", "get up", "up"] {
            assert_eq!(Emote::from_text(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_request_carries_what_each_body_would_play() {
        let chassis = Entity::PLACEHOLDER;
        let asked = |text| request_for(chassis, text).map(|r| (r.emote, r.beast, r.winged));
        assert_eq!(
            asked("hello"),
            Some((
                Some(Emote::Greeting),
                Some(BeastEmote::Wag),
                Some(WingedEmote::WingStretch)
            ))
        );
        assert_eq!(
            asked("sit"),
            Some((None, Some(BeastEmote::Sit), Some(WingedEmote::Rest)))
        );
        assert_eq!(asked("no"), Some((Some(Emote::Reject), None, None)));
        // A winged body's own word asks nothing of a person or a beast.
        assert_eq!(asked("perch"), Some((None, None, Some(WingedEmote::Rest))));
        assert_eq!(asked("look at that mountain"), None);
    }

    #[test]
    fn no_two_beast_emotes_claim_the_same_word() {
        for (index, emote) in BeastEmote::ALL.into_iter().enumerate() {
            let others = BeastEmote::ALL.into_iter().skip(index + 1);
            for other in others {
                for word in emote.keywords() {
                    assert!(
                        !other.keywords().contains(word),
                        "{word:?} is claimed by both {emote:?} and {other:?}"
                    );
                }
            }
            for word in emote.keywords() {
                assert!(
                    !Emote::Reject.keywords().contains(word),
                    "{word:?} is a refusal, and {emote:?} plays it"
                );
            }
        }
    }

    #[test]
    fn each_body_is_told_its_own_words() {
        use crate::pds::avatar::locomotion::beast_record;
        use symbios_avatar::plan::Family;
        let beast = BeastEmote::hint_line();
        for emote in BeastEmote::ALL {
            let word = emote.keywords()[0];
            assert!(beast.contains(word), "hint dropped {word:?}: {beast}");
            assert_eq!(BeastEmote::from_text(word), Some(emote), "{word:?}");
        }
        assert_eq!(hint_for(Some(&beast_record(Family::Dog))), beast);
        let person = crate::pds::AvatarRecord::default_for_did("did:plc:hint-test");
        assert_eq!(hint_for(Some(&person)), Emote::hint_line());
        assert_eq!(hint_for(None), Emote::hint_line());
    }

    /// **A winged body hears the owner's words** (#1878): a person's and a
    /// beast's where it has a gesture for them, nothing for a refusal, which
    /// still decides its sentence, and nine of its own.
    #[test]
    fn a_winged_body_answers_the_owners_words() {
        use WingedEmote::*;
        for (text, answer) in [
            ("hello", Some(WingStretch)),
            ("Hi there!", Some(WingStretch)),
            ("stretch", Some(WingStretch)),
            ("yes", Some(HeadTilt)),
            ("thanks", Some(Preen)),
            ("preen", Some(Preen)),
            ("groom", Some(Preen)),
            ("sit", Some(Rest)),
            ("perch", Some(Rest)),
            ("rest", Some(Rest)),
            ("Curl!", Some(Rest)),
            ("lie down", Some(Roost)),
            ("lay down", Some(Roost)),
            ("roost", Some(Roost)),
            ("shake", Some(Shake)),
            ("get up", Some(GetUp)),
            ("up", Some(GetUp)),
            ("perch, thanks", Some(Rest)),
            // Nothing to play for a refusal, and it decides its sentence.
            ("no", None),
            ("no thanks", None),
            ("nope, roost", None),
            // Whole words, and the whole of a phrase.
            ("lie", None),
            ("restless", None),
            ("curled", None),
            ("the weather is fine", None),
            ("", None),
        ] {
            assert_eq!(WingedEmote::from_text(text), answer, "{text:?}");
        }
    }

    /// **Every winged word plays a gesture its family builds** (#1878), by the
    /// engine's own roster and its own resolver, on a rig of each family and
    /// of a cross led by each: the owner's table, the rests a bird's perch, a
    /// bat's rest and a glider's curl, a roost a bird's own and a bat's or a
    /// glider's lying down its rest.
    #[test]
    fn every_winged_word_names_a_gesture_its_family_builds() {
        use crate::pds::avatar::RiggedKind;
        use symbios_avatar::anim::gesture;
        use symbios_avatar::anim::winged::rests;
        use symbios_avatar::{Archetype, WingedFamily, WingedParams};
        let families = [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider];
        for (family, roster, rest) in [
            (WingedFamily::Bird, rests::BIRD, "Perch"),
            (WingedFamily::Bat, rests::BAT, "Rest"),
            (WingedFamily::Glider, rests::GLIDER, "Curl"),
        ] {
            for emote in WingedEmote::ALL {
                if let Some(name) = emote.gesture_name(family) {
                    assert!(roster.contains(&name), "{family:?}: {emote:?} names {name}");
                }
            }
            assert_eq!(WingedEmote::Rest.gesture_name(family), Some(rest));
            let lies = WingedEmote::Roost.gesture_name(family);
            let roost = if family == WingedFamily::Bird {
                "Roost"
            } else {
                rest
            };
            assert_eq!(lies, Some(roost), "{family:?}");
            // On a rig of the family alone, and of a cross it leads.
            let other = families[(families.iter().position(|f| *f == family).unwrap() + 1) % 3];
            for blend in [0.0, 0.3] {
                let mut params = WingedParams::default();
                params.set_kind(family, Some(other), blend);
                params.blend = blend;
                let body = crate::pds::avatar::EngineAvatarRecord {
                    archetype: Archetype::Winged(params),
                    ..Default::default()
                };
                assert_eq!(RiggedKind::of_record(&body), RiggedKind::Winged(family));
                let config = symbios_avatar::AvatarConfig {
                    atlas: 16,
                    ..Default::default()
                };
                let rig = symbios_avatar::Avatar::build_with(&body, &config)
                    .expect("builds")
                    .rig;
                assert_eq!(gesture::roster(&rig), roster, "{family:?} at {blend}");
                for emote in WingedEmote::ALL {
                    if let Some(name) = emote.gesture_name(family) {
                        assert!(
                            gesture::for_body(&rig, name).is_some(),
                            "{family:?} at {blend}: {name} does not build"
                        );
                    }
                }
            }
        }
    }

    /// **No word is claimed twice, and none is a refusal** (#1878).
    #[test]
    fn no_two_winged_emotes_claim_the_same_word() {
        for (index, emote) in WingedEmote::ALL.into_iter().enumerate() {
            for other in WingedEmote::ALL.into_iter().skip(index + 1) {
                for word in emote.keywords() {
                    assert!(
                        !other.keywords().any(|theirs| theirs == word),
                        "{word:?} is claimed by both {emote:?} and {other:?}"
                    );
                }
            }
            for word in emote.keywords() {
                assert!(
                    !Emote::Reject.keywords().contains(&word),
                    "{word:?} is a refusal, and {emote:?} plays it"
                );
            }
        }
    }

    /// **A winged body is told its own words** (#1878): each family the words
    /// that play on it, each gesture once - not a person's line, nor a
    /// beast's, nor a bird's roost told to a bat, whose lying down is its
    /// rest - and a person and a beast what they always were.
    #[test]
    fn a_winged_body_is_told_its_own_words() {
        use crate::pds::avatar::locomotion::{default_winged, fitted_record};
        use symbios_avatar::WingedFamily;
        for family in [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider] {
            let hint = hint_for(Some(&fitted_record(default_winged(family))));
            assert_eq!(hint, WingedEmote::hint_line(family), "{family:?}");
            assert_ne!(hint, Emote::hint_line(), "{family:?} told a person's words");
            assert_ne!(
                hint,
                BeastEmote::hint_line(),
                "{family:?} told a beast's words"
            );
            let named = hint
                .trim_start_matches("Say ")
                .split(" - ")
                .next()
                .expect("a list");
            let mut told = 0;
            for word in named.split(", ") {
                let emote = WingedEmote::from_text(word).expect(word);
                assert!(
                    emote.gesture_name(family).is_some(),
                    "{family:?} is told {word:?}, which plays nothing on it"
                );
                told += 1;
            }
            let mut plays: Vec<&str> = WingedEmote::ALL
                .iter()
                .filter_map(|emote| emote.gesture_name(family))
                .collect();
            plays.sort_unstable();
            plays.dedup();
            let plays = plays.len();
            assert_eq!(told, plays, "{family:?}: {hint}");
        }
        assert!(WingedEmote::hint_line(WingedFamily::Bird).contains("roost"));
        assert!(!WingedEmote::hint_line(WingedFamily::Bat).contains("roost"));
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
