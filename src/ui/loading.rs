//! Loading-screen progress panel.
//!
//! `AppState::Loading` gates on six tasks (heightmap, room / avatar /
//! inventory record fetches, ambient-audio bake, room compile — see
//! [`crate::loading::check_loading_complete`]), and a slow PDS
//! round-trip can hold the gate for many seconds while the fetch
//! machinery retries with exponential backoff. A bare spinner gives the
//! user no way to tell "still working" from "stuck", so this panel
//! lists each gate task with its live status, including the retry
//! countdown the backoff markers carry.
//!
//! Everything shown here is read straight from the same ECS state the
//! gate itself checks: a row is *done* exactly when the resource the
//! gate waits on is present, and *retrying* exactly while a
//! [`PendingRecordRetry`] marker exists for that record type.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::diagnostics::anomaly::LoadingClock;
use crate::diagnostics::anomaly::rules::GATE_STALL_SECS;
use crate::diagnostics::event::FetchStatus;
use crate::loading::AmbientHandle;
use crate::loading::fetch::{
    LoadedRecord, PendingRecordRetry, RecordFetchOutcomes, is_failure_fallback,
};
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::state::{LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord};
use crate::terrain::FinishedHeightMap;

/// Gate-elapsed past which the countdown turns amber ("slower than a healthy
/// load"); it turns red at the D-engine critical stall threshold
/// [`GATE_STALL_SECS`]. A normal login load settles in a few seconds.
const GATE_WARN_SECS: f64 = 15.0;

/// Colour + suffix for the live gate-elapsed line by how long the gate has been
/// open. Pure over `elapsed` so it unit-tests without egui.
fn gate_elapsed_style(elapsed: f64) -> (egui::Color32, &'static str) {
    if elapsed >= GATE_STALL_SECS {
        (egui::Color32::from_rgb(220, 90, 90), " — stalled")
    } else if elapsed >= GATE_WARN_SECS {
        (
            egui::Color32::from_rgb(210, 170, 90),
            " — slower than usual",
        )
    } else {
        (egui::Color32::LIGHT_GRAY, "")
    }
}

/// Display state of one gate task.
enum RowStatus {
    /// The gate resource is present.
    Done,
    /// The gate resource is present, but only because the fetch fell
    /// back to the default after a FAILURE (decode error / exhausted
    /// retries) — rendered amber, not as a green success (#840). A 404
    /// default (fresh account) still counts as [`RowStatus::Done`].
    Fallback,
    /// Work is in flight (fetching / generating / baking).
    Active,
    /// A transient fetch failure is waiting out its backoff window.
    Retrying {
        attempt: u32,
        max: u32,
        in_secs: f64,
    },
}

/// Derive a record row's status from its gate resource, its terminal
/// fetch outcome and retry markers. The fetch task itself doesn't need
/// probing: while neither the resource nor a retry marker exists the
/// fetch is in flight (the start systems dispatch on the first Loading
/// frame).
fn record_row<R: LoadedRecord>(
    resource_present: bool,
    outcome: Option<FetchStatus>,
    retries: &Query<&PendingRecordRetry<R>>,
    now: f64,
) -> RowStatus {
    if resource_present {
        return if outcome.is_some_and(is_failure_fallback) {
            RowStatus::Fallback
        } else {
            RowStatus::Done
        };
    }
    if let Some(marker) = retries.iter().next() {
        return RowStatus::Retrying {
            attempt: marker.attempt(),
            max: R::MAX_ATTEMPTS,
            in_secs: (marker.fire_at_secs() - now).max(0.0),
        };
    }
    RowStatus::Active
}

/// One labelled status line: check-mark, spinner, or retry countdown.
fn draw_row(ui: &mut egui::Ui, label: &str, status: RowStatus) {
    ui.horizontal(|ui| {
        match status {
            RowStatus::Done => {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "✔");
                ui.label(label);
            }
            RowStatus::Fallback => {
                let amber = egui::Color32::from_rgb(210, 170, 90);
                ui.colored_label(amber, "⚠");
                ui.label(label);
                ui.colored_label(amber, "— using default (stored copy unavailable)");
            }
            RowStatus::Active => {
                ui.spinner();
                ui.label(label);
            }
            RowStatus::Retrying {
                attempt,
                max,
                in_secs,
            } => {
                ui.spinner();
                ui.label(label);
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "retrying in {:.0}s (attempt {attempt}/{max})",
                        in_secs.ceil()
                    ),
                );
            }
        };
    });
}

/// Render the loading screen: heading plus one live status row per gate
/// task. Registered in `crate::run` under `EguiPrimaryContextPass`
/// while in `AppState::Loading`.
#[allow(clippy::too_many_arguments)]
pub fn loading_ui(
    mut contexts: EguiContexts,
    heightmap: Option<Res<FinishedHeightMap>>,
    live_room: Option<Res<LiveRoomRecord>>,
    live_avatar: Option<Res<LiveAvatarRecord>>,
    live_inventory: Option<Res<LiveInventoryRecord>>,
    ambient: Option<Res<AmbientHandle>>,
    world_compiled: Option<Res<crate::world_builder::WorldCompiled>>,
    room_retries: Query<&PendingRecordRetry<RoomRecord>>,
    avatar_retries: Query<&PendingRecordRetry<AvatarRecord>>,
    inventory_retries: Query<&PendingRecordRetry<InventoryRecord>>,
    outcomes: Res<RecordFetchOutcomes>,
    time: Res<Time>,
    loading_clock: Res<LoadingClock>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let now = time.elapsed_secs_f64();

    let terrain_status = if heightmap.is_some() {
        RowStatus::Done
    } else {
        RowStatus::Active
    };
    let room_status =
        record_row::<RoomRecord>(live_room.is_some(), outcomes.room, &room_retries, now);
    let avatar_status =
        record_row::<AvatarRecord>(live_avatar.is_some(), outcomes.avatar, &avatar_retries, now);
    let inventory_status = record_row::<InventoryRecord>(
        live_inventory.is_some(),
        outcomes.inventory,
        &inventory_retries,
        now,
    );
    // The ambient bake only dispatches once the room record lands, so
    // until then the row is genuinely waiting on the room row above —
    // shown as active anyway: from the user's perspective the
    // soundscape *is* still being worked on.
    let ambient_status = if ambient.is_some() {
        RowStatus::Done
    } else {
        RowStatus::Active
    };
    // The compile pass needs the heightmap-backed terrain mesh and the
    // room record before it can run (see the WorldBuilderPlugin
    // registration note); until then it is genuinely waiting on the
    // rows above — shown active anyway, same rationale as ambient.
    let world_status = if world_compiled.is_some() {
        RowStatus::Done
    } else {
        RowStatus::Active
    };

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            // Push the block toward the vertical centre without
            // `centered_and_justified` (which would stack the rows on
            // one line).
            ui.add_space(ui.available_height() * 0.35);
            ui.heading("Generating the overlands…");
            // Live loading-gate countdown (C-5): how long the gate has been open,
            // amber past the warn point and red at the D critical stall threshold.
            if let Some(entered) = loading_clock.entered_at() {
                let elapsed = (now - entered).max(0.0);
                let (color, note) = gate_elapsed_style(elapsed);
                ui.colored_label(color, format!("Loading gate: {elapsed:.0}s{note}"));
            }
            ui.add_space(12.0);
            // Fixed-width child so the six rows left-align with each
            // other while the block as a whole stays centred.
            ui.allocate_ui(egui::vec2(340.0, 0.0), |ui| {
                ui.vertical(|ui| {
                    draw_row(ui, "Terrain heightmap", terrain_status);
                    draw_row(ui, "World recipe (room record)", room_status);
                    draw_row(ui, "Avatar record", avatar_status);
                    draw_row(ui, "Inventory", inventory_status);
                    draw_row(ui, "Ambient soundscape", ambient_status);
                    draw_row(ui, "Building world", world_status);
                });
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_elapsed_style_thresholds() {
        // Fresh load → neutral (no suffix).
        assert_eq!(gate_elapsed_style(0.0).1, "");
        assert_eq!(gate_elapsed_style(GATE_WARN_SECS - 0.1).1, "");
        // Past the warn point → amber.
        assert_eq!(gate_elapsed_style(GATE_WARN_SECS).1, " — slower than usual");
        assert_eq!(
            gate_elapsed_style(GATE_STALL_SECS - 0.1).1,
            " — slower than usual"
        );
        // Past the D critical stall threshold → red.
        assert_eq!(gate_elapsed_style(GATE_STALL_SECS).1, " — stalled");
    }
}
