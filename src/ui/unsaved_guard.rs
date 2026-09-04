//! Confirm-dialog guard against silently losing unpublished record edits.
//!
//! Two flows used to discard in-flight edits without warning: walking
//! through an inter-room portal replaces [`LiveRoomRecord`] /
//! [`StoredRoomRecord`] with the destination owner's record, and logging
//! out tears down every record resource. Both now route through
//! [`UnsavedGuard`]: the portal contact handler and the Log out button
//! insert the resource instead of acting directly, and [`unsaved_guard_ui`]
//! decides what happens next.
//!
//! The guard is deliberately the *single* owner of the dirty logic:
//!
//! - **Clean case** — nothing relevant differs from its stored mirror, so
//!   the guard proceeds on the very next frame without rendering anything.
//!   Callers therefore never need the record resources themselves; they
//!   unconditionally open the guard.
//! - **Dirty case** — a modal offers *Publish & continue* (spawns the same
//!   publish tasks the editors use, waits for every poll to drain, then
//!   re-checks), *Discard & continue*, or *Stay*.
//!
//! Waiting for the publish to finish before acting is load-bearing, not
//! politeness: the publish poll systems pin `stored = live` **at completion
//! time**, so letting portal travel swap [`LiveRoomRecord`] while a publish
//! is in flight would pin the *destination's* record as the local user's
//! stored mirror the moment the task resolved.
//!
//! Which records are "relevant" depends on the action: portal travel only
//! swaps the room record (avatar and inventory ride along), so only room
//! dirt blocks it — and only when the local user actually owns the room
//! they are standing in. Logout discards everything, so room (owner only),
//! avatar and inventory all count.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::diagnostics::event::RecordKind;
use crate::oauth::OauthRefreshCtx;
use crate::pds::avatar::avatar_is_dirty;
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::player::{PortalCooldown, begin_portal_travel};
use crate::state::{
    AppState, CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord,
    PublishFeedback, PublishStatus, StoredAvatarRecord, StoredInventoryRecord, StoredRoomRecord,
    records_differ,
};
use crate::ui::avatar::PublishAvatarTask;
use crate::ui::editable::RecoveryMarkers;
use crate::ui::inventory::PublishInventoryTask;
use crate::ui::room::{PublishRoomTask, ResetRoomTask};

/// How long portal interaction stays suppressed after the player chooses
/// *Stay*. Longer than the post-teleport [`PortalCooldown`] default: the
/// player is standing inside the portal collider when they decline, and
/// the wider window gives them time to walk clear before the overlap
/// re-opens the dialog.
const DECLINE_COOLDOWN_SECS: f64 = 3.0;

/// What the guard will do once the dirty question is settled.
#[derive(Clone, Debug)]
pub enum GuardedAction {
    /// Begin the async room-record fetch that carries the player to
    /// another overland (see `player::begin_portal_travel`). `target_pos:
    /// None` arrives at the destination record's `default_landing` (#745).
    PortalTravel {
        target_did: String,
        target_pos: Option<Vec3>,
    },
    /// Transition back to `AppState::Login`; `logout::cleanup_on_logout`
    /// does the actual teardown on the state edge.
    Logout,
    /// Close the app (native window-close intercept, #839): the window's
    /// close button routes through this guard instead of killing the
    /// process with unsaved edits aboard. Confirming exits via `AppExit`.
    Quit,
}

/// Dialog lifecycle. `Publishing` renders a spinner and waits for every
/// outstanding publish task to drain before re-checking the dirty set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardPhase {
    Prompt,
    Publishing,
}

/// Why the dialog is asking again after a publish it waited on (#1206).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardNotice {
    /// A publish this guard waited on failed; the record's own status
    /// line said so and this is what it said.
    PublishFailed(String),
    /// Every publish this guard waited on drained without a failure, and
    /// the relevant records are still dirty — an edit made while the save
    /// was in flight, which `stored` (pinned to what was WRITTEN, #1116)
    /// correctly does not cover. Not a failure, and not said as one.
    StillDirty,
}

impl GuardNotice {
    /// The dialog line.
    pub fn text(&self) -> String {
        match self {
            Self::PublishFailed(error) => format!("Publish failed — {error}"),
            Self::StillDirty => String::from(
                "The save finished, but unsaved edits remain — publish again, or discard them.",
            ),
        }
    }
}

/// Present while a guarded action is pending. Inserted by the portal
/// contact handler and the Log out button; removed by [`unsaved_guard_ui`]
/// when the action proceeds or the user backs out (and defensively by
/// `logout::cleanup_on_logout`).
#[derive(Resource, Debug)]
pub struct UnsavedGuard {
    pub action: GuardedAction,
    pub phase: GuardPhase,
    /// Why the dialog is asking again, surfaced so the user understands.
    pub notice: Option<GuardNotice>,
    /// When this guard entered `Publishing` — the clock a publish outcome
    /// must postdate to be quoted as THIS attempt's (#1206). A `Failed`
    /// status is never reset by an edit, so without this a save that
    /// failed minutes earlier was reported as the reason this attempt
    /// failed.
    pub publishing_since: Option<f64>,
}

impl UnsavedGuard {
    pub fn new(action: GuardedAction) -> Self {
        Self {
            action,
            phase: GuardPhase::Prompt,
            notice: None,
            publishing_since: None,
        }
    }

    fn enter_publishing(&mut self, now: f64) {
        self.phase = GuardPhase::Publishing;
        self.publishing_since = Some(now);
    }
}

/// The dialog's button labels for one action, so the three actions stay
/// in step and the Publishing phase's one non-destructive exit is named
/// for what it does (#1206): it does not continue the action, it closes
/// the dialog and lets the save land — it used to read "Continue in
/// background", which sat where "Stay here" sits and read as "proceed".
pub struct GuardLabels {
    pub publish: &'static str,
    pub discard: &'static str,
    pub stay: &'static str,
    /// `stay`, qualified for the Publishing phase.
    pub stay_while_publishing: String,
}

pub fn guard_labels(action: &GuardedAction) -> GuardLabels {
    let (publish, discard, stay) = match action {
        GuardedAction::PortalTravel { .. } => ("Publish & travel", "Discard & travel", "Stay here"),
        GuardedAction::Logout => ("Publish & log out", "Discard & log out", "Cancel"),
        GuardedAction::Quit => ("Publish & quit", "Discard & quit", "Cancel"),
    };
    GuardLabels {
        publish,
        discard,
        stay,
        stay_while_publishing: format!("{stay} (save continues)"),
    }
}

/// Which editable records currently differ from their stored mirrors.
/// `room` is owner-gated by the caller: a visitor's live room record
/// legitimately diverges whenever the host edits live, and that is not
/// the visitor's data to save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DirtyRecords {
    pub room: bool,
    pub avatar: bool,
    pub inventory: bool,
}

impl DirtyRecords {
    /// True when this dirty set should block the given action. Portal
    /// travel only replaces the room record; logout discards everything.
    pub(crate) fn blocks(&self, action: &GuardedAction) -> bool {
        match action {
            GuardedAction::PortalTravel { .. } => self.room,
            GuardedAction::Logout | GuardedAction::Quit => {
                self.room || self.avatar || self.inventory
            }
        }
    }

    /// The records "Publish & continue" would write for `action` — the
    /// dirty ones the action discards — that carry a recovery marker
    /// (#1199), with the marker's reason. `reasons` is
    /// `[room, avatar, inventory]` as [`RecoveryMarkers::reasons`] hands
    /// it over. Non-empty means the guard must not offer the publish: it
    /// would write a synthesised default over a stored copy this client
    /// never read, and for an avatar retire every attachment record the
    /// default does not reference.
    pub(crate) fn recovery_blocked<'a>(
        &self,
        action: &GuardedAction,
        reasons: [Option<&'a str>; 3],
    ) -> Vec<(RecordKind, &'a str)> {
        let [room, avatar, inventory] = reasons;
        let mut blocked = Vec::new();
        if self.room
            && let Some(reason) = room
        {
            blocked.push((RecordKind::Room, reason));
        }
        if matches!(action, GuardedAction::Logout | GuardedAction::Quit) {
            if self.avatar
                && let Some(reason) = avatar
            {
                blocked.push((RecordKind::Avatar, reason));
            }
            if self.inventory
                && let Some(reason) = inventory
            {
                blocked.push((RecordKind::Inventory, reason));
            }
        }
        blocked
    }
}

/// The six live/stored record resources the dirty computation diffs.
#[derive(SystemParam)]
pub struct GuardRecords<'w> {
    live_room: Option<Res<'w, LiveRoomRecord>>,
    stored_room: Option<Res<'w, StoredRoomRecord>>,
    live_avatar: Option<Res<'w, LiveAvatarRecord>>,
    stored_avatar: Option<Res<'w, StoredAvatarRecord>>,
    live_inventory: Option<Res<'w, LiveInventoryRecord>>,
    stored_inventory: Option<Res<'w, StoredInventoryRecord>>,
}

impl GuardRecords<'_> {
    /// Diff every live record against its stored mirror. `owns_room`
    /// gates the room diff — see [`DirtyRecords`].
    fn compute(&self, owns_room: bool) -> DirtyRecords {
        let differ_room = owns_room
            && match (&self.live_room, &self.stored_room) {
                (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
                _ => false,
            };
        // Avatar-specific (#1059): a rigged body's payload rides on the
        // serde-skipped `resolved`, so a plain wire compare would call a
        // sculpted body clean and let the logout guard drop the work. One
        // shared derivation with the Save row and Ctrl+S (#1138).
        let differ_avatar = match (&self.live_avatar, &self.stored_avatar) {
            (Some(live), Some(stored)) => avatar_is_dirty(&live.0, &stored.0),
            _ => false,
        };
        let differ_inventory = match (&self.live_inventory, &self.stored_inventory) {
            (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
            _ => false,
        };
        DirtyRecords {
            room: differ_room,
            avatar: differ_avatar,
            inventory: differ_inventory,
        }
    }
}

/// Publish-status lines for the three record types, written when the
/// guard spawns publish tasks and read back for the failure message.
#[derive(SystemParam)]
pub struct GuardFeedbacks<'w> {
    room: ResMut<'w, PublishFeedback<RoomRecord>>,
    avatar: ResMut<'w, PublishFeedback<AvatarRecord>>,
    inventory: ResMut<'w, PublishFeedback<InventoryRecord>>,
}

impl GuardFeedbacks<'_> {
    /// First failure among the record types the action cares about that
    /// postdates `since`, for the dialog's notice line.
    fn failure_message(&self, action: &GuardedAction, since: f64) -> Option<String> {
        let mut sources: Vec<(&str, &PublishStatus)> = vec![("World", &self.room.status)];
        if matches!(action, GuardedAction::Logout | GuardedAction::Quit) {
            sources.push(("Avatar", &self.avatar.status));
            sources.push(("Inventory", &self.inventory.status));
        }
        recent_failure(&sources, since)
    }
}

/// The first `Failed` status stamped at or after `since` (#1206). A
/// status older than the guard's own wait is about some earlier attempt
/// — an edit never resets it — and quoting it as this attempt's reason
/// told the owner a save had failed when nothing of theirs had.
pub(crate) fn recent_failure(sources: &[(&str, &PublishStatus)], since: f64) -> Option<String> {
    sources.iter().find_map(|(label, status)| match status {
        PublishStatus::Failed { at_secs, message } if *at_secs >= since => {
            Some(format!("{label}: {message}"))
        }
        _ => None,
    })
}

/// Which PDS writes are running right now, as data, so the wait rule is
/// a pure function beside [`DirtyRecords::blocks`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InFlightWrites {
    pub room: bool,
    pub avatar: bool,
    pub inventory: bool,
}

impl InFlightWrites {
    /// Whether a running write should hold `action` (#1206) — the same
    /// question [`DirtyRecords::blocks`] answers for dirt. Portal travel
    /// swaps only the room record, so only a room write can pin the wrong
    /// thing when it lands; an inventory write from a gift accepted a
    /// moment ago is irrelevant to it. The guard used to wait on ANY task,
    /// so that gift showed the traveller a spinner labelled "Publishing…",
    /// then "Publish failed — publish did not complete" when it drained.
    pub(crate) fn blocks(&self, action: &GuardedAction) -> bool {
        match action {
            GuardedAction::PortalTravel { .. } => self.room,
            GuardedAction::Logout | GuardedAction::Quit => {
                self.room || self.avatar || self.inventory
            }
        }
    }
}

/// Existence probes for the publish-task components. The guard's
/// `Publishing` phase waits until the ones its action depends on have
/// drained (the editors' poll systems despawn each task entity when its
/// result lands).
///
/// The room's recovery reset is a write too (#1199): it lands through the
/// same poll system and pins `stored` the same way, so a portal hop or a
/// logout on top of it would pin the local default over the destination's
/// record exactly as the module docs say the wait exists to prevent.
#[derive(SystemParam)]
pub struct GuardPublishTasks<'w, 's> {
    room: Query<'w, 's, (), With<PublishRoomTask>>,
    reset: Query<'w, 's, (), With<ResetRoomTask>>,
    avatar: Query<'w, 's, (), With<PublishAvatarTask>>,
    inventory: Query<'w, 's, (), With<PublishInventoryTask>>,
}

impl GuardPublishTasks<'_, '_> {
    fn in_flight(&self) -> InFlightWrites {
        InFlightWrites {
            room: !self.room.is_empty() || !self.reset.is_empty(),
            avatar: !self.avatar.is_empty(),
            inventory: !self.inventory.is_empty(),
        }
    }

    /// Whether a PDS write `action` has to wait for is still running.
    pub fn blocks(&self, action: &GuardedAction) -> bool {
        self.in_flight().blocks(action)
    }
}

/// Render the guard dialog and drive the pending action to a conclusion.
/// Runs in `EguiPrimaryContextPass` while [`UnsavedGuard`] exists (see
/// the registration in `crate::run`).
#[allow(clippy::too_many_arguments)]
pub fn unsaved_guard_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut guard: ResMut<UnsavedGuard>,
    records: GuardRecords,
    mut feedbacks: GuardFeedbacks,
    tasks: GuardPublishTasks,
    recoveries: RecoveryMarkers,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<OauthRefreshCtx>>,
    current_room: Option<Res<CurrentRoomDid>>,
    mut next_state: ResMut<NextState<AppState>>,
    time: Res<Time>,
    // Portal hops are milestone events the analyzer's timeline renders
    // (#1144); this is where a travel is actually committed.
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    let now = time.elapsed_secs_f64();
    let owns_room = match (session.as_deref(), current_room.as_deref()) {
        (Some(session), Some(room)) => session.did == room.0,
        _ => false,
    };
    let dirty = records.compute(owns_room);

    // A publish the user fired from an editor moments before triggering
    // the action is morally the same as clicking "Publish & continue":
    // wait for it rather than racing it or double-publishing. Only a
    // write the action depends on (#1206) — see `InFlightWrites::blocks`.
    if guard.phase == GuardPhase::Prompt
        && dirty.blocks(&guard.action)
        && tasks.blocks(&guard.action)
    {
        guard.enter_publishing(now);
    }

    match guard.phase {
        GuardPhase::Publishing => {
            if tasks.blocks(&guard.action) {
                // Still waiting on at least one poll system to drain its
                // task — render the holding state below.
            } else if !dirty.blocks(&guard.action) {
                // Every relevant publish succeeded (the polls pinned
                // stored = live) — nothing left to lose.
                proceed(
                    &guard.action,
                    &mut commands,
                    &mut next_state,
                    &mut session_log,
                    now,
                );
                return;
            } else {
                // Drained but still dirty: a publish failed, or an edit
                // landed during the flight. Fall back to the prompt with
                // the honest reason — and only a failure from THIS wait.
                let since = guard.publishing_since.unwrap_or(now);
                guard.notice = Some(
                    feedbacks
                        .failure_message(&guard.action, since)
                        .map_or(GuardNotice::StillDirty, GuardNotice::PublishFailed),
                );
                guard.phase = GuardPhase::Prompt;
            }
        }
        GuardPhase::Prompt => {
            if !dirty.blocks(&guard.action) {
                // Clean (or only irrelevant records differ): proceed
                // without ever showing the dialog. This is the everyday
                // path — callers open the guard unconditionally.
                proceed(
                    &guard.action,
                    &mut commands,
                    &mut next_state,
                    &mut session_log,
                    now,
                );
                return;
            }
        }
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let GuardLabels {
        publish: continue_publish,
        discard: continue_discard,
        stay,
        stay_while_publishing,
    } = guard_labels(&guard.action);

    crate::ui::confirm::note_modal_open(ctx);
    egui::Modal::new(egui::Id::new("unsaved-guard")).show(ctx, |ui| {
        ui.heading("Unpublished changes");
        ui.add_space(4.0);

        let mut names: Vec<&str> = Vec::new();
        if dirty.room {
            names.push("World");
        }
        if matches!(guard.action, GuardedAction::Logout | GuardedAction::Quit) {
            if dirty.avatar {
                names.push("Avatar");
            }
            if dirty.inventory {
                names.push("Inventory");
            }
        }
        ui.label(format!(
            "You have unpublished edits to: {}.",
            names.join(", ")
        ));
        ui.label(match guard.action {
            GuardedAction::PortalTravel { .. } => "Traveling through the portal will discard them.",
            GuardedAction::Logout => "Logging out will discard them.",
            GuardedAction::Quit => "Quitting will discard them.",
        });

        if let Some(notice) = &guard.notice {
            ui.add_space(4.0);
            let theme = crate::ui::theme::current(ui.ctx());
            let color = match notice {
                GuardNotice::PublishFailed(_) => theme.status.error,
                GuardNotice::StillDirty => theme.status.warn,
            };
            ui.colored_label(color, notice.text());
        }
        ui.add_space(8.0);

        if guard.phase == GuardPhase::Publishing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Publishing…");
            });
            ui.add_space(4.0);
            // The non-destructive exit is named for what it does (#1206):
            // it closes the dialog WITHOUT the action — backing out doesn't
            // cancel the in-flight tasks, the editors' poll systems land
            // them as a normal publish, so the label says the save keeps
            // going. It used to read "Continue in background", which sat
            // where "Stay here" sits and read as "proceed".
            if ui.button(&stay_while_publishing).clicked() {
                close(&guard.action, &mut commands, &time);
            }
            // Discard stays reachable while publishing (#1129). It was
            // not, and that made a stalled request a trap rather than a
            // failure: the guard auto-enters this phase whenever a task
            // is in flight, the only button was "Continue in background",
            // and the beforeunload guard resists a reload because the
            // record is still dirty. Discarding does not depend on the
            // task — proceeding abandons the edits either way — so there
            // is no reason to withhold it, and the publish timeouts added
            // alongside this only shorten the trap rather than remove it.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(crate::ui::confirm::danger_button(
                        continue_discard,
                        &crate::ui::theme::current(ui.ctx()),
                    ))
                    .clicked()
                {
                    proceed(
                        &guard.action,
                        &mut commands,
                        &mut next_state,
                        &mut session_log,
                        now,
                    );
                }
            });
            return;
        }

        // Publishing needs an authenticated session; without one (which
        // should not happen in-game) only discard/stay are offered. And it
        // must not write a record whose fetch fell back to a default
        // (#1199): that publish is the editor's, behind its confirm.
        let blocked = dirty.recovery_blocked(&guard.action, recoveries.reasons());
        let can_publish = session.is_some() && refresh_ctx.is_some() && blocked.is_empty();
        if !blocked.is_empty() {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.warn,
                crate::ui::editable::publish_blocked_hover(&blocked),
            );
            ui.add_space(4.0);
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_publish, egui::Button::new(continue_publish))
                .on_disabled_hover_text(if blocked.is_empty() {
                    String::from("Publishing needs a signed-in session.")
                } else {
                    crate::ui::editable::publish_blocked_hover(&blocked)
                })
                .clicked()
                && let (Some(session), Some(refresh_ctx)) =
                    (session.as_deref(), refresh_ctx.as_deref())
            {
                guard.notice = None;
                if dirty.room
                    && let Some(live) = records.live_room.as_deref()
                {
                    feedbacks.room.status = PublishStatus::Publishing { since_secs: now };
                    let room_did = current_room
                        .as_deref()
                        .map(|d| d.0.clone())
                        .unwrap_or_default();
                    crate::ui::room::spawn_room_publish_task(
                        &mut commands,
                        session,
                        refresh_ctx,
                        live.0.clone(),
                        room_did,
                        time.elapsed_secs_f64(),
                    );
                }
                if matches!(guard.action, GuardedAction::Logout | GuardedAction::Quit) {
                    if dirty.avatar
                        && let Some(live) = records.live_avatar.as_deref()
                    {
                        feedbacks.avatar.status = PublishStatus::Publishing { since_secs: now };
                        crate::ui::avatar::spawn_publish_avatar_task(
                            &mut commands,
                            session,
                            refresh_ctx,
                            live.0.clone(),
                            // The stored record's references, so this save
                            // retires what it takes off exactly as the
                            // editor's own Publish does (#1110). Passing
                            // nothing here is what orphaned every prop
                            // removed before a "Publish & log out".
                            records
                                .stored_avatar
                                .as_deref()
                                .map_or_else(Vec::new, |stored| {
                                    crate::pds::avatar::wardrobe::attachment_rkeys(&stored.0)
                                }),
                            time.elapsed_secs_f64(),
                        );
                    }
                    if dirty.inventory
                        && let Some(live) = records.live_inventory.as_deref()
                    {
                        feedbacks.inventory.status = PublishStatus::Publishing { since_secs: now };
                        crate::ui::inventory::spawn_publish_inventory_task(
                            &mut commands,
                            session,
                            refresh_ctx,
                            live.0.clone(),
                            records
                                .stored_inventory
                                .as_deref()
                                .map(|s| s.0.clone())
                                .unwrap_or_default(),
                            time.elapsed_secs_f64(),
                        );
                    }
                }
                guard.enter_publishing(now);
            }
            if ui.button(stay).clicked() {
                close(&guard.action, &mut commands, &time);
            }
            // Discard is the data-loss option (#838): danger-styled and
            // pushed to the far edge so it is never adjacent to the two
            // safe choices — the old row rendered three identical
            // buttons side by side.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(crate::ui::confirm::danger_button(
                        continue_discard,
                        &crate::ui::theme::current(ui.ctx()),
                    ))
                    .clicked()
                {
                    // No revert needed: portal travel overwrites the live
                    // room record with the destination's, and logout
                    // removes every record resource outright.
                    proceed(
                        &guard.action,
                        &mut commands,
                        &mut next_state,
                        &mut session_log,
                        now,
                    );
                }
            });
        });
    });
}

/// Execute the guarded action and drop the guard.
fn proceed(
    action: &GuardedAction,
    commands: &mut Commands,
    next_state: &mut NextState<AppState>,
    session_log: &mut crate::diagnostics::SessionLog,
    now: f64,
) {
    match action {
        GuardedAction::PortalTravel {
            target_did,
            target_pos,
        } => {
            begin_portal_travel(commands, session_log, now, target_did.clone(), *target_pos);
        }
        GuardedAction::Logout => {
            next_state.set(AppState::Login);
        }
        GuardedAction::Quit => {
            // `close_when_requested` is disabled so the [x] could route
            // here — exiting is now on us.
            commands.write_message(bevy::app::AppExit::Success);
        }
    }
    commands.remove_resource::<UnsavedGuard>();
}

/// Drop the guard without acting. For a declined portal travel the player
/// is still standing inside the portal collider, so a widened cooldown
/// keeps the overlap from re-opening the dialog before they can walk out.
fn close(action: &GuardedAction, commands: &mut Commands, time: &Time) {
    if matches!(action, GuardedAction::PortalTravel { .. }) {
        commands.insert_resource(PortalCooldown {
            until_secs: time.elapsed_secs_f64() + DECLINE_COOLDOWN_SECS,
        });
    }
    commands.remove_resource::<UnsavedGuard>();
}

// ---------------------------------------------------------------------
// Exit guards (#839): the guard used to cover only portals, gateways and
// logout — closing the native window or the browser tab bypassed it
// entirely and took the unsaved edits down with the process.
// ---------------------------------------------------------------------

/// Native: intercept the window's close button. The `WindowPlugin` is built with
/// `close_when_requested: false`, so nothing closes until this system
/// decides: clean records exit immediately; dirty ones raise the same
/// guard dialog portals and logout use, as [`GuardedAction::Quit`].
/// Runs in every `AppState` — outside `InGame` the record resources are
/// absent, the dirty set is empty, and the close is unprompted.
#[cfg(not(target_arch = "wasm32"))]
pub fn intercept_window_close(
    mut close_requested: MessageReader<bevy::window::WindowCloseRequested>,
    records: GuardRecords,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
    guard: Option<Res<UnsavedGuard>>,
    mut commands: Commands,
    mut exit: MessageWriter<bevy::app::AppExit>,
) {
    if close_requested.is_empty() {
        return;
    }
    close_requested.clear();
    // A guard dialog is already up (possibly mid-publish) — a second [x]
    // must not bypass it.
    if guard.is_some() {
        return;
    }
    let owns_room = matches!(
        (session.as_deref(), current_room.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    if records.compute(owns_room).blocks(&GuardedAction::Quit) {
        commands.insert_resource(UnsavedGuard::new(GuardedAction::Quit));
    } else {
        exit.write(bevy::app::AppExit::Success);
    }
}

/// wasm: the live "would closing this tab lose work?" bit for the
/// `beforeunload` listener. A JS event handler can't query the ECS, so
/// [`sync_beforeunload_dirty`] mirrors the dirty state here and the
/// listener just reads it.
#[cfg(target_arch = "wasm32")]
static BEFOREUNLOAD_DIRTY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// How often the wasm dirty mirror recomputes. The diff serializes all
/// three records, so it is throttled rather than per-frame; a one-second
/// stale window on a browser-close prompt is imperceptible.
#[cfg(target_arch = "wasm32")]
const BEFOREUNLOAD_SYNC_INTERVAL_SECS: f64 = 1.0;

/// wasm: throttled mirror of the guard's derived dirty set into
/// [`BEFOREUNLOAD_DIRTY`]. Runs in every `AppState`; with the record
/// resources absent (login screen) the flag settles to `false` and the
/// tab closes unprompted.
#[cfg(target_arch = "wasm32")]
pub fn sync_beforeunload_dirty(
    records: GuardRecords,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
    time: Res<Time>,
    mut next_check: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next_check {
        return;
    }
    *next_check = now + BEFOREUNLOAD_SYNC_INTERVAL_SECS;
    let owns_room = matches!(
        (session.as_deref(), current_room.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    let dirty = records.compute(owns_room).blocks(&GuardedAction::Quit);
    BEFOREUNLOAD_DIRTY.store(dirty, std::sync::atomic::Ordering::Relaxed);
}

/// wasm: install the `beforeunload` listener once at startup. While the
/// mirrored dirty flag is set, closing/reloading the tab raises the
/// browser's own leave-site confirm; while clean it does nothing at all
/// (no `preventDefault`, no return value — an unconditional handler
/// would nag on every navigation). Leaked via `Closure::forget`: it must
/// live for the whole page lifetime anyway.
#[cfg(target_arch = "wasm32")]
pub fn install_beforeunload_guard() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let Some(window) = web_sys::window() else {
        return;
    };
    let closure = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
        move |event: web_sys::BeforeUnloadEvent| {
            if BEFOREUNLOAD_DIRTY.load(std::sync::atomic::Ordering::Relaxed) {
                // Modern browsers ignore the string and show their own
                // wording; preventDefault + a non-empty return value is
                // the cross-browser way to request the prompt.
                event.prevent_default();
                event.set_return_value("You have unsaved edits.");
            }
        },
    );
    if let Err(e) =
        window.add_event_listener_with_callback("beforeunload", closure.as_ref().unchecked_ref())
    {
        warn!("failed to install beforeunload guard: {e:?}");
    }
    closure.forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirty(room: bool, avatar: bool, inventory: bool) -> DirtyRecords {
        DirtyRecords {
            room,
            avatar,
            inventory,
        }
    }

    fn travel() -> GuardedAction {
        GuardedAction::PortalTravel {
            target_did: "did:plc:example".into(),
            target_pos: Some(Vec3::ZERO),
        }
    }

    #[test]
    fn portal_travel_only_blocks_on_room_dirt() {
        assert!(dirty(true, false, false).blocks(&travel()));
        // Avatar and inventory survive a portal hop — they must not gate it.
        assert!(!dirty(false, true, true).blocks(&travel()));
        assert!(!dirty(false, false, false).blocks(&travel()));
    }

    /// #1199. Sequence: the avatar fetch fell back to the default (marker
    /// raised, live == stored == default), the owner nudges one axis, then
    /// logs out and picks "Publish & log out". That publish wrote the
    /// default over the stored avatar AND retired every attachment record
    /// the default does not reference. The guard must refuse the publish
    /// for exactly the dirty records that carry a marker — and only those.
    #[test]
    fn publish_and_continue_is_refused_for_a_dirty_record_in_recovery() {
        let avatar_reason = [None, Some("timed out"), None];
        let blocked =
            dirty(false, true, false).recovery_blocked(&GuardedAction::Logout, avatar_reason);
        assert_eq!(blocked, vec![(RecordKind::Avatar, "timed out")]);
        // A marker on a CLEAN record blocks nothing: the guard would not
        // publish it, so there is nothing to clobber.
        assert!(
            dirty(true, false, false)
                .recovery_blocked(&GuardedAction::Logout, avatar_reason)
                .is_empty()
        );
        // Portal travel publishes the room only, so avatar/inventory
        // markers are irrelevant to it …
        assert!(
            dirty(true, true, true)
                .recovery_blocked(&travel(), avatar_reason)
                .is_empty()
        );
        // … while a room marker on a dirty room blocks it.
        let room_reason = [Some("decode error"), None, None];
        assert_eq!(
            dirty(true, false, false).recovery_blocked(&travel(), room_reason),
            vec![(RecordKind::Room, "decode error")]
        );
        // Every marker on every dirty record is named, in record order.
        let all = [Some("r"), Some("a"), Some("i")];
        assert_eq!(
            dirty(true, true, true).recovery_blocked(&GuardedAction::Quit, all),
            vec![
                (RecordKind::Room, "r"),
                (RecordKind::Avatar, "a"),
                (RecordKind::Inventory, "i")
            ]
        );
    }

    /// #1206, finding 196, the pure half. The write that holds an action
    /// is decided the way the dirt that blocks it is: per action.
    #[test]
    fn a_write_holds_only_the_actions_that_would_pin_it_wrong() {
        let inventory_only = InFlightWrites {
            room: false,
            avatar: false,
            inventory: true,
        };
        assert!(!inventory_only.blocks(&travel()));
        assert!(inventory_only.blocks(&GuardedAction::Logout));
        assert!(inventory_only.blocks(&GuardedAction::Quit));
        let room_only = InFlightWrites {
            room: true,
            avatar: false,
            inventory: false,
        };
        assert!(room_only.blocks(&travel()));
        assert!(!InFlightWrites::default().blocks(&GuardedAction::Logout));
    }

    /// #1206, finding 196, the stale-quote half. Sequence: a save fails at
    /// t=10 (the status keeps `Failed`; nothing resets it), the owner edits
    /// on, then at t=600 travels; a room write that was in flight drains
    /// clean but an edit made during it leaves the room dirty. The guard
    /// used to quote the t=10 failure as this attempt's reason. Only a
    /// failure stamped since the guard began waiting counts, and with none
    /// the notice is "still dirty", not "failed".
    #[test]
    fn the_guard_quotes_only_a_failure_from_its_own_wait() {
        let old = PublishStatus::Failed {
            at_secs: 10.0,
            message: String::from("502 Bad Gateway"),
        };
        let sources = [("World", &old)];
        assert_eq!(recent_failure(&sources, 600.0), None);
        assert_eq!(
            recent_failure(&sources, 5.0),
            Some(String::from("World: 502 Bad Gateway"))
        );
        // The wait started the same second the failure landed: quoted.
        assert!(recent_failure(&sources, 10.0).is_some());
        assert_eq!(
            GuardNotice::StillDirty.text(),
            String::from(
                "The save finished, but unsaved edits remain — publish again, or discard them."
            )
        );
    }

    /// #1206, finding 195. "Continue in background" sat where "Stay here"
    /// sits, in the phase whose only other button is the red Discard, and
    /// read as "proceed" — it closed the dialog and dropped the action.
    /// The non-destructive exit now carries the action's own stay verb and
    /// says what happens to the save.
    #[test]
    fn the_publishing_phase_exit_is_named_for_staying() {
        assert_eq!(
            guard_labels(&travel()).stay_while_publishing,
            "Stay here (save continues)"
        );
        assert_eq!(
            guard_labels(&GuardedAction::Logout).stay_while_publishing,
            "Cancel (save continues)"
        );
        assert_eq!(guard_labels(&GuardedAction::Quit).discard, "Discard & quit");
    }

    #[test]
    fn logout_blocks_on_any_dirt() {
        assert!(dirty(true, false, false).blocks(&GuardedAction::Logout));
        assert!(dirty(false, true, false).blocks(&GuardedAction::Logout));
        assert!(dirty(false, false, true).blocks(&GuardedAction::Logout));
        assert!(!dirty(false, false, false).blocks(&GuardedAction::Logout));
    }
}
