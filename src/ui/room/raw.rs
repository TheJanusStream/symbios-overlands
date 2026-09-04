//! Raw JSON tab — fallback editor that always round-trips whatever the
//! visual tabs don't yet expose.
//!
//! The buffer knows what it was seeded from (#1212). Arriving at the tab
//! used to re-serialise the record into the text box unconditionally, so a
//! flick to Placements and back replaced half-finished hand edits with the
//! record — the one tab where the longest, least reproducible editing
//! happens, and the loss was not on the undo stack (Ctrl+Z restores the
//! record, not the text box). Now every record change asks
//! [`RawJsonBuffer::sync_to`], which re-seeds only when the text is still
//! what it was seeded from, and otherwise keeps the edits and says so.

use bevy_egui::egui;

use crate::pds::RoomRecord;

/// The Raw tab's text and where it came from.
#[derive(Default)]
pub(super) struct RawJsonBuffer {
    /// What the owner sees and types into.
    text: String,
    /// The serialisation `text` was last seeded from. `text != seeded_from`
    /// is exactly "there are unparsed edits".
    seeded_from: String,
    /// The record changed underneath unparsed edits (a Revert, Reset,
    /// re-roll or undo restore) — said in the tab, since Parse would apply
    /// the old text over the new record.
    stale: bool,
    error: Option<String>,
    initialised: bool,
}

impl RawJsonBuffer {
    fn serialise(record: &RoomRecord) -> String {
        serde_json::to_string_pretty(record).unwrap_or_else(|e| format!("// serialize error: {e}"))
    }

    /// Whether the text differs from what it was seeded from.
    pub(super) fn is_edited(&self) -> bool {
        self.text != self.seeded_from
    }

    /// Replace the text with the record's serialisation, discarding any
    /// edits — the explicit "Discard and refresh".
    pub(super) fn reseed(&mut self, record: &RoomRecord) {
        self.text = Self::serialise(record);
        self.seeded_from = self.text.clone();
        self.stale = false;
        self.error = None;
        self.initialised = true;
    }

    /// The record changed (tab arrival, Revert, Reset, re-roll, undo): show
    /// it — unless the owner has unparsed edits, which are kept and flagged
    /// stale instead of being overwritten.
    pub(super) fn sync_to(&mut self, record: &RoomRecord) {
        if !self.initialised || !self.is_edited() {
            self.reseed(record);
            return;
        }
        // Edits in hand: stale only if the record really moved — a tab
        // switch over an unchanged record is just unparsed, and saying
        // "the record changed underneath" there would cry wolf.
        if Self::serialise(record) != self.seeded_from {
            self.stale = true;
        }
    }

    /// First-draw seed; a no-op once initialised.
    pub(super) fn ensure_seeded(&mut self, record: &RoomRecord) {
        if !self.initialised {
            self.reseed(record);
        }
    }
}

/// The one-line statement of the wire convention (#1212, finding 57). The
/// tab serialises the record exactly as the PDS stores it, and that is
/// not what any other tab shows: a decimal literal is a hard type error.
/// Rotations are quaternions, and angle fields use the units their names
/// say — not a blanket "radians", which would be wrong for most of them.
pub(super) const WIRE_CONVENTION: &str = "Numbers are shown as the wire stores them: decimals are \
     whole numbers scaled by 10 000 (1.5 is written 15000; a decimal point is a parse error), 64-bit \
     seeds are quoted strings, rotations are quaternions [x, y, z, w] scaled the same way, and angle \
     fields use the unit their name says (…_deg in degrees; tilt/twist in radians).";

/// Rows the editor shows before its own scroll bar takes over. The whole
/// record sits in one un-virtualised `TextEdit` — egui lays out the full
/// galley — so the box is bounded here rather than growing to the record's
/// full height inside the tab's outer scroll (finding 62).
const EDITOR_ROWS: usize = 24;

pub(super) fn draw_raw_tab(
    ui: &mut egui::Ui,
    raw: &mut RawJsonBuffer,
    pending: &mut RoomRecord,
    dirty: &mut bool,
    label: &mut crate::ui::undo::LabelSlot,
) {
    ui.heading("Raw JSON");
    ui.add_space(4.0);
    ui.label("Advanced escape hatch: every field the visual tabs do not expose. Parse errors abort the commit.");
    ui.label(
        egui::RichText::new(WIRE_CONVENTION)
            .small()
            .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
    ui.add_space(4.0);
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    egui::ScrollArea::vertical()
        .id_salt("raw_json_editor")
        .max_height(row_height * EDITOR_ROWS as f32)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut raw.text)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_rows(EDITOR_ROWS)
                    .desired_width(f32::INFINITY),
            );
        });
    if let Some(err) = raw.error.as_ref() {
        ui.colored_label(crate::ui::theme::current(ui.ctx()).status.error, err);
    }
    let edited = raw.is_edited();
    if edited {
        let theme = crate::ui::theme::current(ui.ctx());
        ui.colored_label(
            theme.status.warn,
            if raw.stale {
                "Unparsed edits — and the record changed underneath them (a revert, reset, \
                 re-roll or undo). Parse applies this text over the current record; Discard \
                 shows the current record."
            } else {
                "Unparsed edits — not in the record until you Parse. Switching tabs keeps them."
            },
        );
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(edited, egui::Button::new("Parse into pending record"))
            .on_disabled_hover_text("The text matches the record — nothing to parse")
            .clicked()
        {
            match serde_json::from_str::<RoomRecord>(&raw.text) {
                Ok(mut parsed) => {
                    // Enforce the same bounds the network-ingress path
                    // applies — the raw JSON tab otherwise lets the owner
                    // bypass `sanitize()` and hand a 2 GiB grid_size or
                    // unbounded L-system iterations straight to the world
                    // compiler.
                    parsed.sanitize();
                    *pending = parsed;
                    *dirty = true;
                    label.set("raw JSON parse");
                    // The record is now what was typed (modulo sanitize):
                    // re-seed so the buffer reads clean and shows what the
                    // sanitiser kept.
                    raw.reseed(pending);
                }
                Err(e) => raw.error = Some(format!("Invalid JSON schema: {e}")),
            }
        }
        let discard = if edited {
            "Discard edits and refresh"
        } else {
            "Refresh from record"
        };
        if ui.button(discard).clicked() {
            raw.reseed(pending);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> RoomRecord {
        RoomRecord::default_for_did("did:plc:raw-json-test")
    }

    /// #1212, finding 56. Sequence: hand-edit a generator in Raw JSON,
    /// flick to Placements, come back. Arrival re-serialised the record
    /// into the box unconditionally and the half-finished JSON was gone —
    /// not on the undo stack, since Ctrl+Z restores the record, not the
    /// text. Arrival now re-seeds only a clean buffer.
    #[test]
    fn a_tab_switch_keeps_unparsed_edits() {
        let record = record();
        let mut raw = RawJsonBuffer::default();
        raw.ensure_seeded(&record);
        assert!(!raw.is_edited());
        // Arriving with a clean buffer refreshes it — the pre-#1212 reason
        // the re-seed existed (edits made in other tabs show up).
        raw.sync_to(&record);
        assert!(!raw.is_edited());

        raw.text.push_str("\n// half-finished");
        assert!(raw.is_edited());
        raw.sync_to(&record);
        assert!(
            raw.text.ends_with("// half-finished"),
            "the edits survive the tab switch"
        );
        assert!(!raw.stale, "same record: not stale, just unparsed");

        // The record changed underneath (Revert / Reset / undo): the edits
        // still survive, and the buffer says the ground moved.
        let mut other = record.clone();
        other.traits.insert("moved".into(), vec!["x".into()]);
        raw.sync_to(&other);
        assert!(raw.is_edited());
        assert!(raw.stale);

        // Discard is the explicit way out, and reads clean again.
        raw.reseed(&other);
        assert!(!raw.is_edited());
        assert!(!raw.stale);
    }

    /// The convention the tab states (finding 57): the ×10 000 integer
    /// rule and the seed-as-string rule are true of the serialisation the
    /// tab shows, and "angles are radians" is NOT stated as a blanket rule.
    #[test]
    fn the_caption_states_the_wire_convention_the_text_actually_uses() {
        let text = RawJsonBuffer::serialise(&record());
        assert!(
            !text.contains("\"scale\": [\n        1.0"),
            "no decimals on the wire"
        );
        assert!(
            text.contains("10000") || text.contains("15000"),
            "scaled integers are what the tab shows"
        );
        assert!(WIRE_CONVENTION.contains("10 000"));
        assert!(WIRE_CONVENTION.contains("quoted"));
        assert!(WIRE_CONVENTION.contains("quaternion"));
        assert!(!WIRE_CONVENTION.contains("angles are radians"));
    }

    /// Finding 62 asked for a measurement before acting: the seeded default
    /// room's pretty JSON, which is what the un-virtualised editor lays out.
    #[test]
    fn the_seeded_room_json_is_measured() {
        let text = RawJsonBuffer::serialise(&record());
        let lines = text.lines().count();
        println!(
            "seeded room pretty JSON: {} bytes, {lines} lines",
            text.len()
        );
        assert!(lines > 100, "a seeded room is not trivially small");
    }
}
