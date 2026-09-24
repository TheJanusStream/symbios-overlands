//! What the interface channel may read and do (#1424).
//!
//! Settled with the owner: the agent works in an allow-list of windows,
//! and inside them a refusal list names the controls whose work belongs to
//! a gate the interface knows nothing of - or to nobody the agent may be.
//! Both lists are written the way a person finds the controls: by window
//! title and label.
//!
//! **Reading.** A window off the list is never read: not its words and not
//! its layout, only its title - which the game writes - so the agent can be
//! told it is there. Chat holds every line anyone in the room said (the game
//! keeps them for the window, `daemon/observe.rs`), where the agent hears
//! its admin's only (#1427). Diagnostics' event log names what other players
//! sent. The gateway picker is the one surface that shows Bluesky display
//! names, which never reach the agent (#1427).
//!
//! **Doing.** A control that saves, deletes a saved record, leaves the
//! world, logs out, mutes a player, writes the operator's clipboard or opens
//! their browser is refused, and the refusal names the command that does
//! the work where its gate can see it: `agent save` for a Save button
//! (`--allow-save`), `agent travel` for Visit (the unsaved-edits rule). What
//! is left is a person's live edits to the agent's own world, avatar and
//! inventory - the class P3 left free - and settings kept in the agent's own
//! directory. The source sweep at the end of this file keeps the list
//! complete: a new call anywhere in the interface that saves, browses,
//! copies, travels, logs out or mutes fails it until it is looked at here.

use serde_json::{Value, json};

use crate::ui::layout::UiWindow;
use crate::ui::shortcuts::window_title;

use super::tree::Kind;

/// Why the agent may not read or work something, and what does the work
/// instead when anything may.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Refusal {
    pub why: &'static str,
    pub instead: Option<&'static str>,
}

impl Refusal {
    /// The refusal as one sentence, for an error.
    pub(super) fn sentence(self, what: &str) -> String {
        match self.instead {
            Some(instead) => format!("{what} is refused: {}; {instead}", self.why),
            None => format!("{what} is refused: {}", self.why),
        }
    }

    pub(super) fn to_json(self) -> Value {
        json!({ "why": self.why, "instead": self.instead })
    }
}

/// The toolbar windows the agent reads and works in.
pub(super) const WINDOWS: [UiWindow; 7] = [
    UiWindow::People,
    UiWindow::Avatar,
    UiWindow::Inventory,
    UiWindow::Catalogue,
    UiWindow::WorldEditor,
    UiWindow::Settings,
    UiWindow::Controls,
];

/// How the audio pop-out's title begins: it is titled after the slot it
/// edits, and opens from an Edit audio button inside the editors above.
pub(super) const AUDIO_EDITOR: &str = "Audio Editor - ";

/// Whether the window titled `title` is one the agent may read.
pub(super) fn readable(title: &str) -> bool {
    WINDOWS.iter().any(|window| window_title(*window) == title) || title.starts_with(AUDIO_EDITOR)
}

/// A toolbar window by the name the agent gives it: its title or its
/// layout key, in any case, `_` and spaces alike.
pub(super) fn window_named(name: &str) -> Option<UiWindow> {
    let wanted = name.trim().to_lowercase().replace('_', " ");
    ALL_WINDOWS.into_iter().find(|window| {
        window_title(*window).to_lowercase() == wanted || window.key().replace('_', " ") == wanted
    })
}

/// Every toolbar window, for [`window_named`] - the refused ones too, so a
/// refused name is refused rather than unknown.
const ALL_WINDOWS: [UiWindow; 10] = [
    UiWindow::Chat,
    UiWindow::People,
    UiWindow::Avatar,
    UiWindow::Inventory,
    UiWindow::Catalogue,
    UiWindow::WorldEditor,
    UiWindow::Diagnostics,
    UiWindow::AudioEditor,
    UiWindow::Controls,
    UiWindow::Settings,
];

/// Why the agent may not open `window` from the toolbar, if it may not.
/// The audio pop-out is not refused - it has no toolbar button, and opens
/// from inside an editor (`ui open` says so).
pub(super) fn open_refusal(window: UiWindow) -> Option<Refusal> {
    match window {
        UiWindow::Chat => Some(CHAT),
        UiWindow::Diagnostics => Some(DIAGNOSTICS),
        UiWindow::AudioEditor
        | UiWindow::People
        | UiWindow::Avatar
        | UiWindow::Inventory
        | UiWindow::Catalogue
        | UiWindow::WorldEditor
        | UiWindow::Settings
        | UiWindow::Controls => None,
    }
}

const CHAT: Refusal = Refusal {
    why: "the chat window holds every line anyone in the room said, and the agent hears its \
          admin's only",
    instead: Some("`agent say` speaks, and `agent events` carries the admin's lines"),
};

const DIAGNOSTICS: Refusal = Refusal {
    why: "its event log names what other players sent",
    instead: Some("`agent status` says where the agent is and who is there"),
};

const GATEWAY: Refusal = Refusal {
    why: "it shows Bluesky display names, which never reach the agent",
    instead: Some("`agent travel <player>` goes where it goes"),
};

/// Whether the window titled `title` holds other players' words or display
/// names - Chat, Diagnostics, the gateway picker. A picture of the
/// interface would show them, so none is taken while one is open. (The
/// travel overlay and the portal prompt, which the agent does not read
/// either, show the game's words and a handle, which the agent may.)
pub(super) fn holds_others_words(title: &str) -> bool {
    matches!(title, "Chat" | "Diagnostics" | "Gateway")
}

/// Why an open window titled `title` is not the agent's to read.
pub(super) fn window_refusal(title: &str) -> Refusal {
    match title {
        "Chat" => CHAT,
        "Diagnostics" => DIAGNOSTICS,
        "Gateway" => GATEWAY,
        _ => Refusal {
            why: "it is not one of the windows the agent works in",
            instead: None,
        },
    }
}

/// The game's own dialogs, by their egui ids: the unsaved-edits guard
/// (before a trip or a logout), sign-in-again, a gift offer (never drawn in
/// the daemon) and the other-session choice. None is ever the agent's to
/// read or answer, whatever it clicked just before one came up.
const GAME_DIALOGS: [(&str, &str); 4] = [
    ("unsaved-guard", "the unsaved-edits dialog"),
    ("session-expired", "the sign-in-again dialog"),
    ("incoming-item-offer", "a gift offer"),
    ("other-session-room", "the other-session dialog"),
];

/// What the game's own dialog with egui id value `id` is, if it is one.
pub(super) fn game_dialog(id: u64) -> Option<&'static str> {
    GAME_DIALOGS
        .iter()
        .find(|(egui_id, _)| bevy_egui::egui::Id::new(*egui_id).value() == id)
        .map(|(_, what)| *what)
}

/// Saving, whichever editor's button it is.
const SAVES: Refusal = Refusal {
    why: "it saves to the agent's account, which only `agent start --allow-save` permits",
    instead: Some("`agent save [room|avatar|inventory]` saves, refused without --allow-save"),
};

const CLIPBOARD: Refusal = Refusal {
    why: "it writes the operator's clipboard",
    instead: None,
};

const BROWSER: Refusal = Refusal {
    why: "it opens a browser on the operator's desktop",
    instead: None,
};

const MUTING: Refusal = Refusal {
    why: "whom the agent hears is its operator's to choose, and muting its admin would \
          silence the one player it listens to",
    instead: None,
};

const LEAVING: Refusal = Refusal {
    why: "it leaves this world without the unsaved-edits rule `agent travel` keeps",
    instead: Some("`agent travel <player>` goes, refused over unsaved edits unless told"),
};

const LOGGING_OUT: Refusal = Refusal {
    why: "logging out revokes the saved session the agent runs on",
    instead: Some("`agent stop` ends the agent and keeps the session"),
};

/// A refused control: the window it is refused in (`None`: every window)
/// and its label, exactly as the game draws it.
struct Refused {
    window: Option<&'static str>,
    label: &'static str,
    refusal: Refusal,
}

/// The refusal list. A label is matched whole, so "Copy to inventory" -
/// which copies into the inventory, not the clipboard - is not "Copy id".
const REFUSED: &[Refused] = &[
    // The three editors' Save row (`ui::editable`), and its after-a-
    // failed-load variant.
    Refused {
        window: None,
        label: "Save",
        refusal: SAVES,
    },
    Refused {
        window: None,
        label: "Save anyway",
        refusal: SAVES,
    },
    // The World Editor's recovery banner: deletes the saved world first.
    Refused {
        window: None,
        label: "Reset stored world",
        refusal: Refusal {
            why: "it deletes the saved world from the agent's account and saves a new one",
            instead: None,
        },
    },
    Refused {
        window: Some("People"),
        label: "Visit",
        refusal: LEAVING,
    },
    // A People row's menu: "Copy account id" and "Open Bluesky profile".
    Refused {
        window: Some("People"),
        label: "\u{2026}",
        refusal: Refusal {
            why: "its menu writes the operator's clipboard and opens their browser",
            instead: None,
        },
    },
    Refused {
        window: Some("People"),
        label: "Mute",
        refusal: MUTING,
    },
    // Settings' muted-player list. The World Editor's "Unmute" lifts the
    // app-wide sound mute, which a daemon born silent does not hear.
    Refused {
        window: Some("Settings"),
        label: "Unmute",
        refusal: MUTING,
    },
    Refused {
        window: Some("Settings"),
        label: "Copy id",
        refusal: CLIPBOARD,
    },
    // The audio pop-out's export (bevy_symbios_audio), through egui's own
    // clipboard.
    Refused {
        window: None,
        label: "Copy JSON",
        refusal: CLIPBOARD,
    },
    // Reachable from no readable window today; named so that one that
    // grows them is refused rather than trusted.
    Refused {
        window: None,
        label: "Copy account id",
        refusal: CLIPBOARD,
    },
    Refused {
        window: None,
        label: "Open Bluesky profile",
        refusal: BROWSER,
    },
    Refused {
        window: None,
        label: "Feedback \u{2197}",
        refusal: BROWSER,
    },
    Refused {
        window: None,
        label: "Log out",
        refusal: LOGGING_OUT,
    },
    Refused {
        window: None,
        label: "Cancel and log out",
        refusal: LOGGING_OUT,
    },
    Refused {
        window: None,
        label: "Travel to my world",
        refusal: LEAVING,
    },
    // The unsaved-edits guard's answers. The guard is one of the game's own
    // dialogs, never the agent's; named so that no path to them opens.
    Refused {
        window: None,
        label: "Save & travel",
        refusal: SAVES,
    },
    Refused {
        window: None,
        label: "Discard & travel",
        refusal: LEAVING,
    },
    Refused {
        window: None,
        label: "Save & log out",
        refusal: LOGGING_OUT,
    },
    Refused {
        window: None,
        label: "Discard & log out",
        refusal: LOGGING_OUT,
    },
];

/// Why the control `kind` labelled `label` in `window` is refused, if it
/// is. `window` is the window a dialog or a menu was raised from.
pub(super) fn control_refusal(window: &str, kind: Kind, label: Option<&str>) -> Option<Refusal> {
    if kind == Kind::Link {
        return Some(BROWSER);
    }
    let label = label?;
    REFUSED
        .iter()
        .find(|refused| refused.label == label && refused.window.is_none_or(|only| only == window))
        .map(|refused| refused.refusal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The windows are named as a person sees them, and by their layout
    /// keys; a refused window is found, so that it can be refused by name.
    #[test]
    fn a_window_is_named_by_its_title_or_its_key() {
        assert_eq!(window_named("World Editor"), Some(UiWindow::WorldEditor));
        assert_eq!(window_named("world_editor"), Some(UiWindow::WorldEditor));
        assert_eq!(window_named(" avatar "), Some(UiWindow::Avatar));
        assert_eq!(window_named("chat"), Some(UiWindow::Chat));
        assert_eq!(window_named("gateway"), None);
        assert!(open_refusal(UiWindow::Chat).is_some());
        assert!(open_refusal(UiWindow::Diagnostics).is_some());
        assert!(open_refusal(UiWindow::Inventory).is_none());
    }

    /// Readable is the allow-list and the audio pop-out, by title; the
    /// windows that carry other players' words are not on it - and are the
    /// ones a picture waits on.
    #[test]
    fn only_the_allow_list_is_readable() {
        for window in WINDOWS {
            assert!(readable(window_title(window)), "{window:?}");
            assert!(!holds_others_words(window_title(window)), "{window:?}");
        }
        for words in ["Chat", "Diagnostics", "Gateway"] {
            assert!(holds_others_words(words) && !readable(words), "{words}");
        }
        assert!(!holds_others_words("portal-prompt"));
        assert!(readable("Audio Editor - Soundtrack"));
        for refused in ["Chat", "Diagnostics", "Gateway", "travel-overlay", ""] {
            assert!(!readable(refused), "{refused}");
        }
    }

    /// Refusals match whole labels in their window: Save anywhere, Visit
    /// and Mute in People, Unmute in Settings but not the World Editor's
    /// sound banner, and never "Copy to inventory".
    #[test]
    fn a_refusal_matches_its_whole_label_in_its_window() {
        let refused = |window, label| control_refusal(window, Kind::Button, Some(label)).is_some();
        assert!(refused("Avatar", "Save"));
        assert!(refused("Inventory", "Save anyway"));
        assert!(refused("People", "Visit"));
        assert!(refused("People", "\u{2026}"));
        assert!(refused("Settings", "Unmute"));
        assert!(
            !refused("World Editor", "Unmute"),
            "the app-wide sound mute"
        );
        assert!(!refused("Catalogue", "Copy to inventory"));
        assert!(!refused("Avatar", "Save as copy"));
        assert!(!refused("Avatar", "Revert to saved"));
        assert!(
            control_refusal("Avatar", Kind::Link, Some("anything")).is_some(),
            "a link opens a browser"
        );
    }
}

/// The refusal list is held complete from the source (#1424): every call in
/// the interface that saves, deletes a saved record, opens a browser,
/// writes the clipboard, travels, logs out or mutes is counted per file, and
/// a count that moves fails this test until someone has looked at the new
/// call and put its control on the list above - or its window on the
/// refused ones - and updated the count.
#[cfg(test)]
mod sweep {
    use crate::ui::fonts::glyph_coverage_tests::{code_only, non_test_source, rust_sources_under};

    /// What a call does, and the text that makes it.
    const EFFECTS: &[(&str, &str)] = &[
        ("saves", "spawn_room_publish_task("),
        ("saves", "spawn_publish_avatar_task("),
        ("saves", "spawn_publish_inventory_task("),
        ("deletes the saved world", "spawn_reset_task("),
        ("opens a browser", "open_url_in_browser("),
        ("opens a browser", "external_link_button("),
        ("opens a browser", "webbrowser::open"),
        ("opens a browser", "open_url("),
        ("opens a browser", ".hyperlink("),
        ("opens a browser", "Hyperlink::"),
        ("writes the clipboard", ".copy("),
        ("writes the clipboard", "copy_text("),
        ("logs out", "GuardedAction::Logout"),
        ("travels", "GuardedAction::PortalTravel"),
        ("travels", "begin_portal_travel("),
        ("mutes", "set_peer_mute("),
    ];

    /// Every such call, file by file, as of #1424, and the control behind
    /// it: in a window the agent may not read, on the refusal list, or not
    /// a control at all.
    const KNOWN: &[(&str, &str, usize)] = &[
        // The helpers themselves: `open_url_in_browser` and the link button
        // built on it, and the spawn functions' definitions.
        ("src/ui/affordances.rs", "external_link_button(", 1),
        ("src/ui/affordances.rs", "open_url_in_browser(", 2),
        ("src/ui/affordances.rs", "webbrowser::open", 1),
        ("src/ui/room/publish.rs", "spawn_room_publish_task(", 1),
        ("src/ui/room/publish.rs", "spawn_reset_task(", 1),
        // Avatar, Inventory, World Editor: one definition-or-call each for
        // the Save row's Save and its recovery "Save anyway" - both
        // refused - and the World Editor's "Reset stored world", refused.
        ("src/ui/avatar/mod.rs", "spawn_publish_avatar_task(", 2),
        (
            "src/ui/inventory/mod.rs",
            "spawn_publish_inventory_task(",
            2,
        ),
        ("src/ui/room/mod.rs", "spawn_room_publish_task(", 1),
        ("src/ui/room/mod.rs", "spawn_reset_task(", 1),
        // People: Visit, the row's menu (Copy account id, Open Bluesky
        // profile) and Mute - refused. Its second mute and its inventory
        // save are the offer dialog's, which the daemon never draws.
        ("src/ui/people.rs", ".copy(", 1),
        ("src/ui/people.rs", "GuardedAction::PortalTravel", 1),
        ("src/ui/people.rs", "open_url_in_browser(", 1),
        ("src/ui/people.rs", "set_peer_mute(", 2),
        ("src/ui/people.rs", "spawn_publish_inventory_task(", 1),
        // Settings: the muted list's Unmute and Copy id - refused.
        ("src/ui/settings.rs", ".copy(", 1),
        ("src/ui/settings.rs", "set_peer_mute(", 1),
        // The toolbar, which the agent never works: Log out, Travel to my
        // world, Feedback - refused by label besides. (The Controls window
        // drawn in the same file has none of them.)
        ("src/ui/toolbar.rs", "GuardedAction::Logout", 1),
        ("src/ui/toolbar.rs", "GuardedAction::PortalTravel", 1),
        ("src/ui/toolbar.rs", "external_link_button(", 1),
        // Refused windows: Chat (its Mute menu), Diagnostics (its copies),
        // the gateway picker (its Go buttons).
        ("src/ui/chat.rs", "set_peer_mute(", 1),
        ("src/ui/diagnostics.rs", ".copy(", 4),
        ("src/ui/gateway.rs", "GuardedAction::PortalTravel", 2),
        // The unsaved-edits dialog - the game's own, never the agent's -
        // and the portal's raising of it, which no control does.
        ("src/ui/unsaved_guard.rs", "GuardedAction::Logout", 9),
        ("src/ui/unsaved_guard.rs", "GuardedAction::PortalTravel", 7),
        ("src/ui/unsaved_guard.rs", "begin_portal_travel(", 1),
        ("src/ui/unsaved_guard.rs", "spawn_publish_avatar_task(", 1),
        (
            "src/ui/unsaved_guard.rs",
            "spawn_publish_inventory_task(",
            1,
        ),
        ("src/ui/unsaved_guard.rs", "spawn_room_publish_task(", 1),
        ("src/ui/travel.rs", "GuardedAction::PortalTravel", 1),
        // The login screen: never the agent's.
        ("src/ui/login/begin.rs", "webbrowser::open", 1),
        ("src/ui/login/mod.rs", ".copy(", 1),
        ("src/ui/login/mod.rs", "external_link_button(", 1),
        ("src/ui/login/mod.rs", "open_url_in_browser(", 2),
    ];

    #[test]
    fn every_interface_call_with_an_outside_effect_is_accounted_for() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found: Vec<(String, &str, usize)> = Vec::new();
        for dir in ["src/ui", "src/editor_gizmo"] {
            for path in rust_sources_under(dir) {
                let text = std::fs::read_to_string(&path).expect("a source file reads");
                let code = code_only(non_test_source(&text));
                let file = path
                    .strip_prefix(root)
                    .expect("under the crate")
                    .to_string_lossy()
                    .replace('\\', "/");
                for (_, needle) in EFFECTS {
                    let count = code.matches(needle).count();
                    if count > 0 {
                        found.push((file.clone(), needle, count));
                    }
                }
            }
        }
        found.sort();
        let mut known: Vec<(String, &str, usize)> = KNOWN
            .iter()
            .map(|(file, needle, count)| ((*file).to_owned(), *needle, *count))
            .collect();
        known.sort();
        let effect = |needle: &str| {
            EFFECTS
                .iter()
                .find(|(_, n)| *n == needle)
                .map_or("does something outside", |(what, _)| *what)
        };
        let new: Vec<String> = found
            .iter()
            .filter(|f| !known.contains(f))
            .map(|(file, needle, count)| {
                format!("{file}: {count} x `{needle}` ({})", effect(needle))
            })
            .collect();
        let gone: Vec<String> = known
            .iter()
            .filter(|k| !found.contains(k))
            .map(|(file, needle, count)| format!("{file}: {count} x `{needle}`"))
            .collect();
        assert!(
            new.is_empty() && gone.is_empty(),
            "the interface's calls with an outside effect moved.\n  new or changed: {new:#?}\n  \
             no longer there: {gone:#?}\nA control the agent can reach that saves, deletes a \
             saved record, opens a browser, writes the clipboard, travels, logs out or mutes \
             must be on the refusal list in src/agent/daemon/ui/policy.rs (or its window \
             refused); then update KNOWN."
        );
    }
}
