//! Loading-screen progress panel.
//!
//! `AppState::Loading` gates on six tasks (heightmap, room / avatar /
//! inventory record fetches, ambient-audio bake, room compile — see
//! [`crate::loading::check_loading_complete`]), and a slow PDS
//! round-trip can hold the gate for many seconds while the fetch
//! machinery retries with exponential backoff. A bare spinner gives the
//! user no way to tell "still working" from "stuck", so this panel
//! lists each gate task with its live status, including the retry
//! countdown + last failure reason the backoff markers carry.
//!
//! Everything shown here is read straight from the same ECS state the
//! gate itself checks: a row is *done* exactly when the resource the
//! gate waits on is present, *retrying* exactly while a
//! [`PendingRecordRetry`] marker exists for that record type, and
//! *waiting* while its upstream dependency (everything funnels through
//! the room record) hasn't landed yet — no fake-busy spinners (#849).
//!
//! The panel is also the escape hatch: "Retry now" short-circuits a
//! backoff window, and "Back to login" aborts the whole pass via
//! [`crate::loading::AbortLoading`] — before #849 a dead PDS could only
//! be escaped by killing the app.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::diagnostics::anomaly::LoadingClock;
use crate::diagnostics::anomaly::rules::GATE_STALL_SECS;
use crate::diagnostics::event::FetchStatus;
use crate::loading::AmbientHandle;
use crate::loading::fetch::{
    LoadedRecord, PendingRecordRetry, RecordFetchOutcomes, RecordFetchTask, is_failure_fallback,
    spawn_record_fetch,
};
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::state::{CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord};
use crate::terrain::FinishedHeightMap;

/// Gate-elapsed past which the countdown turns amber ("slower than a healthy
/// load"); it turns red at the D-engine critical stall threshold
/// [`GATE_STALL_SECS`]. A normal login load settles in a few seconds.
const GATE_WARN_SECS: f64 = 15.0;

/// Row-block width. Wide enough that a retrying row (spinner + label +
/// countdown + "Retry now") stays on one line — the old 340 px wrapped
/// it into a jumble (#849).
const ROWS_WIDTH: f32 = 470.0;

/// Colour + suffix for the live elapsed line by how long the gate has been
/// open. Pure over `elapsed` so it unit-tests without egui. The red tier
/// deliberately does NOT say "stalled": with the room fetch's ~10-minute
/// retry budget, a load past [`GATE_STALL_SECS`] is usually still making
/// (slow) progress, and the per-row status is the honest detail (#849).
fn gate_elapsed_style(elapsed: f64, th: &crate::ui::theme::Theme) -> (egui::Color32, &'static str) {
    if elapsed >= GATE_STALL_SECS {
        (th.status.error, " — much longer than usual")
    } else if elapsed >= GATE_WARN_SECS {
        (th.status.warn, " — slower than usual")
    } else {
        (th.text_strong, "")
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
    /// Work is in flight (fetching / generating / baking). Carries the
    /// `(attempt, max)` pair while a retry attempt is in flight, so the
    /// counter stays visible through the marker-despawn gap between a
    /// retry firing and its task resolving (#849).
    Active(Option<(u32, u32)>),
    /// Not started because an upstream dependency hasn't landed — shown
    /// as an honest "waiting on …" instead of a fake-busy spinner (#849).
    Blocked(&'static str),
    /// A transient fetch failure is waiting out its backoff window.
    Retrying {
        attempt: u32,
        max: u32,
        in_secs: f64,
        /// What the failed attempt reported, shown small under the row.
        reason: String,
    },
}

/// What the user clicked on a row this frame.
#[derive(PartialEq, Eq)]
enum RowAction {
    None,
    /// Fire the pending retry immediately instead of waiting out the
    /// backoff window.
    RetryNow,
}

/// Derive a record row's status from its gate resource, its terminal
/// fetch outcome, retry markers and the in-flight task's attempt
/// counter. While neither the resource nor a retry marker exists the
/// fetch is in flight (the start systems dispatch on the first Loading
/// frame).
fn record_row<R: LoadedRecord>(
    resource_present: bool,
    outcome: Option<FetchStatus>,
    retry: Option<&PendingRecordRetry<R>>,
    in_flight_attempt: Option<u32>,
    now: f64,
) -> RowStatus {
    if resource_present {
        return if outcome.is_some_and(is_failure_fallback) {
            RowStatus::Fallback
        } else {
            RowStatus::Done
        };
    }
    if let Some(marker) = retry {
        return RowStatus::Retrying {
            attempt: marker.attempt(),
            max: R::MAX_ATTEMPTS,
            in_secs: (marker.fire_at_secs() - now).max(0.0),
            reason: marker.reason().to_string(),
        };
    }
    RowStatus::Active(
        in_flight_attempt
            .filter(|attempt| *attempt > 0)
            .map(|attempt| (attempt, R::MAX_ATTEMPTS)),
    )
}

/// One labelled status line: check-mark, spinner, waiting note, or retry
/// countdown (with its failure reason and a "Retry now" escape hatch).
fn draw_row(ui: &mut egui::Ui, label: &str, status: RowStatus) -> RowAction {
    let mut action = RowAction::None;
    // A retrying row's failure reason renders on its own indented line
    // below the row proper — set inside the closure, drawn after it.
    let mut retry_reason: Option<String> = None;
    ui.horizontal(|ui| {
        match status {
            RowStatus::Done => {
                ui.colored_label(crate::ui::theme::current(ui.ctx()).status.ok, "✔");
                ui.label(label);
            }
            RowStatus::Fallback => {
                let amber = crate::ui::theme::current(ui.ctx()).status.warn;
                ui.colored_label(amber, "⚠");
                ui.label(label);
                ui.colored_label(amber, "— using default (stored copy unavailable)");
            }
            RowStatus::Active(attempt) => {
                ui.spinner();
                ui.label(label);
                if let Some((attempt, max)) = attempt {
                    ui.colored_label(
                        crate::ui::theme::current(ui.ctx()).status.warn,
                        format!("(attempt {attempt}/{max})"),
                    );
                }
            }
            RowStatus::Blocked(on) => {
                ui.label("…");
                ui.label(label);
                ui.weak(format!("— waiting on {on}"));
            }
            RowStatus::Retrying {
                attempt,
                max,
                in_secs,
                reason,
            } => {
                ui.spinner();
                ui.label(label);
                ui.colored_label(
                    crate::ui::theme::current(ui.ctx()).status.warn,
                    format!(
                        "retrying in {:.0}s (attempt {attempt}/{max})",
                        in_secs.ceil()
                    ),
                );
                if ui.small_button("Retry now").clicked() {
                    action = RowAction::RetryNow;
                }
                retry_reason = Some(reason);
            }
        };
    });
    // Surface the failure under the row so "retrying" isn't a mystery;
    // truncated hard (on a char boundary) because FetchError debug
    // strings can carry full URLs.
    if let Some(mut reason) = retry_reason {
        if reason.chars().count() > 90 {
            reason = reason.chars().take(90).collect();
            reason.push('…');
        }
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            ui.weak(egui::RichText::new(reason).small());
        });
    }
    action
}

/// Everything the loading gate itself waits on, bundled as a
/// [`SystemParam`] so [`loading_ui`] stays under Bevy's 16-param
/// `IntoSystem` ceiling.
#[derive(SystemParam)]
pub struct GateState<'w> {
    heightmap: Option<Res<'w, FinishedHeightMap>>,
    live_room: Option<Res<'w, LiveRoomRecord>>,
    live_avatar: Option<Res<'w, LiveAvatarRecord>>,
    live_inventory: Option<Res<'w, LiveInventoryRecord>>,
    ambient: Option<Res<'w, AmbientHandle>>,
    world_compiled: Option<Res<'w, crate::world_builder::WorldCompiled>>,
}

/// Per-record retry markers + in-flight tasks, bundled for the same
/// reason as [`GateState`].
#[derive(SystemParam)]
pub struct RecordRows<'w, 's> {
    room_retries: Query<'w, 's, (Entity, &'static PendingRecordRetry<RoomRecord>)>,
    avatar_retries: Query<'w, 's, (Entity, &'static PendingRecordRetry<AvatarRecord>)>,
    inventory_retries: Query<'w, 's, (Entity, &'static PendingRecordRetry<InventoryRecord>)>,
    room_tasks: Query<'w, 's, &'static RecordFetchTask<RoomRecord>>,
    avatar_tasks: Query<'w, 's, &'static RecordFetchTask<AvatarRecord>>,
    inventory_tasks: Query<'w, 's, &'static RecordFetchTask<InventoryRecord>>,
}

/// Despawn `R`'s pending retry marker(s) and refire the fetch right now —
/// the "Retry now" click. The respawn mirrors
/// [`crate::loading::fetch::fire_pending_record_retries`] exactly; only
/// the deadline check is skipped.
fn retry_now<R: LoadedRecord>(
    commands: &mut Commands,
    retries: &Query<(Entity, &'static PendingRecordRetry<R>)>,
    now: f64,
) {
    for (entity, marker) in retries.iter() {
        commands.entity(entity).despawn();
        spawn_record_fetch::<R>(commands, marker.did().to_string(), marker.attempt(), now);
    }
}

/// Render the loading screen: destination + elapsed heading, one live
/// status row per gate task in dependency order, and the abort escape
/// hatch. Registered in `crate::run` under `EguiPrimaryContextPass`
/// while in `AppState::Loading`.
#[allow(clippy::too_many_arguments)]
pub fn loading_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    gate: GateState,
    rows: RecordRows,
    outcomes: Res<RecordFetchOutcomes>,
    time: Res<Time>,
    loading_clock: Res<LoadingClock>,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let now = time.elapsed_secs_f64();

    let room_status = record_row::<RoomRecord>(
        gate.live_room.is_some(),
        outcomes.room,
        rows.room_retries.iter().next().map(|(_, m)| m),
        rows.room_tasks.iter().next().map(|t| t.attempt()),
        now,
    );
    let avatar_status = record_row::<AvatarRecord>(
        gate.live_avatar.is_some(),
        outcomes.avatar,
        rows.avatar_retries.iter().next().map(|(_, m)| m),
        rows.avatar_tasks.iter().next().map(|t| t.attempt()),
        now,
    );
    let inventory_status = record_row::<InventoryRecord>(
        gate.live_inventory.is_some(),
        outcomes.inventory,
        rows.inventory_retries.iter().next().map(|(_, m)| m),
        rows.inventory_tasks.iter().next().map(|t| t.attempt()),
        now,
    );
    let room_landed = gate.live_room.is_some();
    // Terrain generation, the ambient bake and the world compile all
    // dispatch off the room record ("world recipe"), so until it lands
    // they are honestly *waiting*, not working (#849).
    let terrain_status = if gate.heightmap.is_some() {
        RowStatus::Done
    } else if !room_landed {
        RowStatus::Blocked("the world recipe")
    } else {
        RowStatus::Active(None)
    };
    let ambient_status = if gate.ambient.is_some() {
        RowStatus::Done
    } else if !room_landed {
        RowStatus::Blocked("the world recipe")
    } else {
        RowStatus::Active(None)
    };
    let world_status = if gate.world_compiled.is_some() {
        RowStatus::Done
    } else if !room_landed {
        RowStatus::Blocked("the world recipe")
    } else if gate.heightmap.is_none() {
        RowStatus::Blocked("the terrain heightmap")
    } else {
        // Rendered at least one frame before the first compile slice runs
        // (see `world_builder::WorldCompileArmed`), so this warning is on
        // screen when the wasm main-thread stall hits.
        RowStatus::Active(None)
    };
    let world_building = matches!(world_status, RowStatus::Active(_));

    let any_retrying = [&room_status, &avatar_status, &inventory_status]
        .iter()
        .any(|s| matches!(s, RowStatus::Retrying { .. }));

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            // Push the block toward the vertical centre without
            // `centered_and_justified` (which would stack the rows on
            // one line).
            ui.add_space(ui.available_height() * 0.35);
            // Destination identity: whose overland this loading screen
            // ends in. A friend's world shows the DID — the handle isn't
            // known until their profile loads in-game.
            match (session.as_deref(), current_room.as_deref()) {
                (Some(s), Some(room)) if room.0 == s.did => {
                    ui.heading(format!("Loading your overland — @{}", s.handle));
                }
                (_, Some(room)) => {
                    ui.heading(format!("Loading the overland of {}", room.0));
                }
                _ => {
                    ui.heading("Generating the overlands…");
                }
            }
            // Live elapsed line (C-5): amber past the warn point, red past
            // the D critical stall threshold.
            if let Some(entered) = loading_clock.entered_at() {
                let elapsed = (now - entered).max(0.0);
                let (color, note) =
                    gate_elapsed_style(elapsed, &crate::ui::theme::current(ui.ctx()));
                ui.colored_label(color, format!("Elapsed: {elapsed:.0}s{note}"));
            }
            if any_retrying {
                // State the eventual outcome so a long retry crawl isn't
                // open-ended dread: the budget is finite and the fallback
                // is a playable default (#849).
                ui.weak(
                    "A record server is unreachable — if it stays down, loading \
                     continues with a default in a few minutes. \"Back to login\" \
                     leaves now.",
                );
            }
            ui.add_space(12.0);
            // Fixed-width child so the rows left-align with each other
            // while the block as a whole stays centred. Dependency order:
            // everything below the recipe row waits on it.
            ui.allocate_ui(egui::vec2(ROWS_WIDTH, 0.0), |ui| {
                ui.vertical(|ui| {
                    if draw_row(ui, "World recipe (room record)", room_status)
                        == RowAction::RetryNow
                    {
                        retry_now::<RoomRecord>(&mut commands, &rows.room_retries, now);
                    }
                    draw_row(ui, "Terrain heightmap", terrain_status);
                    if draw_row(ui, "Avatar record", avatar_status) == RowAction::RetryNow {
                        retry_now::<AvatarRecord>(&mut commands, &rows.avatar_retries, now);
                    }
                    if draw_row(ui, "Inventory", inventory_status) == RowAction::RetryNow {
                        retry_now::<InventoryRecord>(&mut commands, &rows.inventory_retries, now);
                    }
                    draw_row(ui, "Ambient soundscape", ambient_status);
                    draw_row(
                        ui,
                        if world_building {
                            // Honest warning: the compile can pause the app
                            // for a few seconds (single-threaded on wasm).
                            "Building world — may pause a few seconds"
                        } else {
                            "Building world"
                        },
                        world_status,
                    );
                });
            });
            ui.add_space(16.0);
            // Escape hatch (#849): abort the pass and return to the login
            // form. One click — a dead PDS must not require killing the app.
            if ui.button("Back to login").clicked() {
                commands.insert_resource(crate::loading::AbortLoading);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_elapsed_style_thresholds() {
        let th = crate::ui::theme::Theme::dark();
        // Fresh load → neutral (no suffix).
        assert_eq!(gate_elapsed_style(0.0, &th).1, "");
        assert_eq!(gate_elapsed_style(GATE_WARN_SECS - 0.1, &th).1, "");
        // Past the warn point → amber.
        assert_eq!(
            gate_elapsed_style(GATE_WARN_SECS, &th).1,
            " — slower than usual"
        );
        assert_eq!(
            gate_elapsed_style(GATE_STALL_SECS - 0.1, &th).1,
            " — slower than usual"
        );
        // Past the D critical stall threshold → red, but NOT the old
        // "stalled" wording: the retry budget runs ~10 minutes, so a slow
        // load is usually still progressing (#849).
        assert_eq!(
            gate_elapsed_style(GATE_STALL_SECS, &th).1,
            " — much longer than usual"
        );
    }
}
