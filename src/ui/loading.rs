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
    LoadedRecord, PendingRecordRetry, RecordFetchOutcomes, RecordFetchTask, fallback_note,
    is_failure_fallback, spawn_record_fetch,
};
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::state::{CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord};
use crate::terrain::FinishedHeightMap;

/// Gate-elapsed past which the countdown turns amber ("slower than a healthy
/// load"); it turns red at the D-engine critical stall threshold
/// [`GATE_STALL_SECS`]. A normal login load settles in a few seconds.
const GATE_WARN_SECS: f64 = 15.0;

/// The escape hatch's label and the cost it now states (#1230 f32).
///
/// Named constants because the honesty is the fix: `abort_loading_to_login`
/// runs the shared logout teardown — token revocation at the user's PDS,
/// and on wasm the persisted session cleared — and the old "Back to login"
/// promised a one-click retry instead.
const ABORT_BUTTON_LABEL: &str = "Cancel and log out";
const ABORT_BUTTON_NOTE: &str = "You'll need to sign in again.";

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
    /// retries / an identity that does not exist) — rendered amber, not
    /// as a green success (#840). A 404 default (fresh account) still
    /// counts as [`RowStatus::Done`]. Carries the note naming which of
    /// them happened (#1230 f22).
    ///
    /// Owned rather than `&'static str` since #1246 f341: the ambient row's
    /// note names the reason a Referenced soundtrack could not be fetched,
    /// which is built from the failure and not from a fixed list.
    Fallback(String),
    /// The fetch succeeded and there is something to say about WHAT it
    /// found (#1232 f28): a 404 at somebody else's DID means they have not
    /// built this yet, and the visitor is standing in a world synthesised
    /// from their identifier. A green tick, because nothing went wrong —
    /// with the note beside it, because "@alice's overland" and "a world
    /// we invented for a DID" are not the same place.
    DoneWithNote(&'static str),
    /// Work is in flight (fetching / generating / baking). Carries the
    /// `(attempt, max)` pair while a retry attempt is in flight, so the
    /// counter stays visible through the marker-despawn gap between a
    /// retry firing and its task resolving (#849).
    Active(Option<(u32, u32)>),
    /// Work is in flight and counts its own units — the world compile
    /// (#1230 f281). The longest phase of the gate was a bare spinner
    /// although the job has counted `units_built` since #351.
    Progress { done: u32, total: u32 },
    /// Not started because an upstream dependency hasn't landed — shown
    /// as an honest "waiting on …" instead of a fake-busy spinner (#849).
    Blocked(&'static str),
    /// The work failed and nothing is retrying it (#1230 f21). Three of
    /// the six rows had no way to say this at all, so the most alarming
    /// loading failure rendered identically to a slow, healthy load —
    /// under an elapsed line that actively reassures.
    Failed(String),
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
    /// Start the failed work again from scratch (#1230 f21) — there is no
    /// pending retry to short-circuit, because nothing was retrying.
    RestartFailed,
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
        if let Some(status) = outcome.filter(|status| is_failure_fallback(*status)) {
            return RowStatus::Fallback(fallback_note(status).to_string());
        }
        if outcome == Some(FetchStatus::NotBuiltYet) {
            return RowStatus::DoneWithNote(fallback_note(FetchStatus::NotBuiltYet));
        }
        return RowStatus::Done;
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
                ui.colored_label(
                    crate::ui::theme::current(ui.ctx()).status.ok,
                    crate::ui::affordances::CHECK,
                );
                ui.label(label);
            }
            RowStatus::DoneWithNote(note) => {
                let theme = crate::ui::theme::current(ui.ctx());
                ui.colored_label(theme.status.ok, crate::ui::affordances::CHECK);
                ui.label(label);
                ui.colored_label(theme.text_weak, note);
            }
            RowStatus::Fallback(note) => {
                let amber = crate::ui::theme::current(ui.ctx()).status.warn;
                ui.colored_label(amber, "⚠");
                ui.label(label);
                ui.colored_label(amber, &note);
            }
            RowStatus::Progress { done, total } => {
                ui.spinner();
                ui.label(label);
                ui.colored_label(
                    crate::ui::theme::current(ui.ctx()).text_weak,
                    format!("{done} of {total}"),
                );
            }
            RowStatus::Failed(reason) => {
                let red = crate::ui::theme::current(ui.ctx()).status.error;
                ui.colored_label(red, crate::ui::affordances::CROSS);
                ui.label(label);
                ui.colored_label(red, "— failed");
                if ui.small_button("Try again").clicked() {
                    action = RowAction::RestartFailed;
                }
                retry_reason = Some(reason);
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
    if let Some(reason) = retry_reason {
        let reason = crate::notify::elide(&reason, 90);
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
    /// Why there is no ambient bed, when the reason is a failure (#1246
    /// f341).
    ambient_failed: Option<Res<'w, crate::loading::AmbientResolveFailed>>,
    world_compiled: Option<Res<'w, crate::world_builder::WorldCompiled>>,
    /// The in-flight sliced compile, for the progress ratio (#1230 f281).
    compile_job: Option<Res<'w, crate::world_builder::compile::CompileJob>>,
    /// Set when the terrain job answered with something that is not a
    /// heightmap (#1230 f21).
    terrain_failed: Option<Res<'w, crate::terrain::TerrainGenFailed>>,
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
    // #1267 f34: the heading printed a stranger's DID verbatim — 32
    // characters, centred, the most prominent text in the app — while
    // every travel surface routes the same value through `travel_label`.
    // The cache is empty for a stranger this early, which is fine: the
    // ladder's last rung elides the identifier instead of printing it
    // whole.
    profiles: Option<Res<crate::avatar::BskyProfileCache>>,
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
    } else if let Some(failed) = gate.terrain_failed.as_deref() {
        // #1230 f21. Until this row existed the failing job re-dispatched
        // itself every frame behind a spinner labelled "working", for the
        // whole session.
        RowStatus::Failed(failed.reason.clone())
    } else if !room_landed {
        RowStatus::Blocked("the world recipe")
    } else {
        RowStatus::Active(None)
    };
    // A failed ambient fetch used to be indistinguishable from a
    // successful bake and from a room with no audio at all, because all
    // three install `AmbientHandle` and the row asked only whether the
    // resource existed (#1246 f341). The visitor stood in total silence
    // under a green check, and so did the owner — the only person who can
    // fix the URL.
    let ambient_status = if let Some(failed) = gate.ambient_failed.as_deref() {
        RowStatus::Fallback(format!("— {}", failed.failure.reason.sentence()))
    } else if gate.ambient.is_some() {
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
        // The job counts its own units (#1230 f281); show them. Before the
        // first slice has planned a queue there is nothing to count, and a
        // bare spinner is the honest answer for that handful of frames.
        // Rendered at least one frame before the first compile slice runs
        // (see `world_builder::WorldCompileArmed`), so the pause warning is
        // on screen when the wasm main-thread stall hits.
        match gate
            .compile_job
            .as_deref()
            .and_then(|job| job.progress())
            .filter(|(_, total)| *total > 0)
        {
            Some((done, total)) => RowStatus::Progress { done, total },
            None => RowStatus::Active(None),
        }
    };
    let world_building = matches!(
        world_status,
        RowStatus::Active(_) | RowStatus::Progress { .. }
    );

    let any_retrying = [&room_status, &avatar_status, &inventory_status]
        .iter()
        .any(|s| matches!(s, RowStatus::Retrying { .. }));

    // egui 0.35 panels show into a `Ui`; a full-screen central panel draws
    // into a screen-sized background layer (bevy_egui 0.41's pattern).
    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        "loading-viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::CentralPanel::default().show(&mut viewport_ui, |ui| {
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
                    ui.heading(format!("Loading your world — @{}", s.handle));
                }
                (_, Some(room)) => {
                    let name = match profiles.as_deref() {
                        Some(cache) => crate::network::presence::travel_label(cache, &room.0, None),
                        None => crate::network::presence::PeerLabel::new(None, Some(&room.0))
                            .addressed(),
                    };
                    ui.heading(format!("Loading {name}'s world"));
                }
                _ => {
                    ui.heading("Generating your world…");
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
                    if draw_row(ui, "Your world's recipe", room_status) == RowAction::RetryNow {
                        retry_now::<RoomRecord>(&mut commands, &rows.room_retries, now);
                    }
                    if draw_row(ui, "Shaping the landscape", terrain_status)
                        == RowAction::RestartFailed
                    {
                        // Dropping the marker is the whole restart: the
                        // start condition it blocks re-fires next frame.
                        commands.remove_resource::<crate::terrain::TerrainGenFailed>();
                    }
                    if draw_row(ui, "Your avatar", avatar_status) == RowAction::RetryNow {
                        retry_now::<AvatarRecord>(&mut commands, &rows.avatar_retries, now);
                    }
                    if draw_row(ui, "Your inventory", inventory_status) == RowAction::RetryNow {
                        retry_now::<InventoryRecord>(&mut commands, &rows.inventory_retries, now);
                    }
                    draw_row(ui, "Composing the soundtrack", ambient_status);
                    draw_row(
                        ui,
                        if world_building {
                            // Honest warning: the compile can pause the app
                            // for a few seconds (single-threaded on wasm).
                            "Building the world — may pause a few seconds"
                        } else {
                            "Building the world"
                        },
                        world_status,
                    );
                });
            });
            ui.add_space(16.0);
            // Escape hatch (#849): abort the pass and return to the login
            // form. One click — a dead PDS must not require killing the app.
            //
            // Named for what it does (#1230 f32). `abort_loading_to_login`
            // runs the shared `logout::cleanup_on_logout`, which fires the
            // RFC 7009 token revocation at the user's PDS and, on wasm,
            // clears the persisted session — so "Back to login" promised a
            // one-click retry and delivered a full re-authentication, to a
            // user who is by definition already frustrated. The teardown is
            // correct; only the promise was wrong.
            if ui.button(ABORT_BUTTON_LABEL).clicked() {
                commands.insert_resource(crate::loading::AbortLoading);
            }
            ui.weak(egui::RichText::new(ABORT_BUTTON_NOTE).small());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1230 f32. The sequence: a user waiting out a slow load clicks "Back
    /// to login" expecting to step back one screen and try again — and
    /// `abort_loading_to_login` runs the shared `logout::cleanup_on_logout`,
    /// which fires the RFC 7009 token revocation at their PDS and, on wasm,
    /// clears the persisted session. The teardown is correct; the label
    /// promised the cheap thing and delivered a full re-authentication, to
    /// somebody already frustrated enough to be hunting for an exit. It is
    /// also the ONLY escape from a stuck load, so the surprise lands on the
    /// user least able to absorb it.
    #[test]
    fn the_only_escape_from_a_stuck_load_says_what_it_costs() {
        assert!(
            ABORT_BUTTON_LABEL.to_lowercase().contains("log out"),
            "{ABORT_BUTTON_LABEL} still promises a step back, not a logout"
        );
        assert!(
            ABORT_BUTTON_NOTE.to_lowercase().contains("sign in again"),
            "{ABORT_BUTTON_NOTE} does not state the cost"
        );
    }

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

    /// THE SEQUENCE (#1232 f28): a visitor follows a link to somebody's
    /// world. That DID has published nothing — perhaps it is not even an
    /// account they use — so the DID-seeded default is synthesised and the
    /// loading row renders a green tick with no note at all. They arrive in
    /// a plausible-looking landscape and have no way to know nobody made
    /// it. The owner's OWN 404 is the zero-configuration homeworld and must
    /// stay exactly as wordless as it was.
    #[test]
    fn a_strangers_empty_repo_is_not_reported_as_a_finished_world() {
        let visited = record_row::<crate::pds::RoomRecord>(
            true,
            Some(FetchStatus::NotBuiltYet),
            None,
            None,
            0.0,
        );
        match visited {
            RowStatus::DoneWithNote(note) => {
                assert!(note.contains("haven't built"), "{note}");
            }
            _ => panic!("expected a noted success"),
        }

        // The owner's own first login: unchanged, and silent.
        assert!(matches!(
            record_row::<crate::pds::RoomRecord>(
                true,
                Some(FetchStatus::NotFound),
                None,
                None,
                0.0
            ),
            RowStatus::Done
        ));
        assert!(fallback_note(FetchStatus::NotFound).is_empty());

        // And it is not a FAILURE: nothing is amber, and nothing offers to
        // reset a repo whose only problem is being empty.
        assert!(!is_failure_fallback(FetchStatus::NotBuiltYet));
    }
}
