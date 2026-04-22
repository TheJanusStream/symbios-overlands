//! Raw JSON tab — fallback editor that always round-trips whatever the
//! visual tabs don't yet expose.

use bevy_egui::egui;

use crate::pds::RoomRecord;

pub(super) fn draw_raw_tab(
    ui: &mut egui::Ui,
    text: &mut String,
    error: &mut Option<String>,
    pending: &mut RoomRecord,
    dirty: &mut bool,
) {
    ui.heading("Raw JSON");
    ui.add_space(4.0);
    ui.label("Advanced escape hatch. Parse errors abort the commit.");
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::multiline(text)
            .font(egui::TextStyle::Monospace)
            .code_editor()
            .desired_rows(18)
            .desired_width(f32::INFINITY),
    );
    if let Some(err) = error.as_ref() {
        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
    }
    ui.horizontal(|ui| {
        if ui.button("Parse into pending record").clicked() {
            match serde_json::from_str::<RoomRecord>(text) {
                Ok(mut parsed) => {
                    // Enforce the same bounds the network-ingress path
                    // applies — the raw JSON tab otherwise lets the owner
                    // bypass `sanitize()` and hand a 2 GiB grid_size or
                    // unbounded L-system iterations straight to the world
                    // compiler.
                    parsed.sanitize();
                    *pending = parsed;
                    *error = None;
                    *dirty = true;
                }
                Err(e) => *error = Some(format!("Invalid JSON schema: {}", e)),
            }
        }
        if ui.button("Refresh from pending").clicked() {
            *text = serde_json::to_string_pretty(pending).unwrap_or_default();
            *error = None;
        }
    });
}

// ---------------------------------------------------------------------------
// Widget helpers
// ---------------------------------------------------------------------------
