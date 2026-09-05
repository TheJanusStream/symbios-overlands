//! In-game UI shell: the top toolbar and the first-run controls hint.
//!
//! Before this existed every panel was a floating egui window that
//! spawned collapsed somewhere over the viewport — discoverable only by
//! noticing its title bar. The toolbar enumerates every panel as a
//! toggle button (so features like the Catalogue or drag-to-gift in
//! People are visible at a glance), and [`UiPanels`] is the single
//! source of truth for which windows are open: each window system reads
//! its flag via `egui::Window::open`, which also gives every window a
//! native close button that writes the flag back.
//!
//! The controls hint covers the other half of the discoverability gap:
//! a first-time visitor landing from a shared link is never told the
//! movement keys. It pops once per session on `InGame` entry and can be
//! re-opened any time from the toolbar's "Controls" button.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::avatar::BskyProfileCache;
use crate::diagnostics::anomaly::InvariantRegistry;
use crate::player::{AirplanePreset, CarPreset, HelicopterPreset, HoverBoatPreset};
use crate::state::{ChatHistory, CurrentRoomDid, LocalPlayer, RemotePeer};
use crate::ui::unsaved_guard::{GuardedAction, UnsavedGuard};

/// Open/closed state for every toolbar-managed window. Initialised at
/// app startup, overwritten by the persisted prefs ([`crate::prefs`],
/// #820) when the machine has saved a layout, and carried across logout
/// so the next session reopens the same panels. Serde: `#[serde(default)]`
/// fills bools missing from an older prefs file with these defaults, so
/// the struct can grow without breaking saved state.
#[derive(Resource, Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct UiPanels {
    pub chat: bool,
    pub people: bool,
    pub avatar: bool,
    pub world_editor: bool,
    pub inventory: bool,
    pub catalogue: bool,
    pub diagnostics: bool,
    /// The Settings window (#857): theme picker + client toggles.
    pub settings: bool,
    /// The controls overlay. Defaults to open — this is the first-run
    /// hint — and is re-openable from the toolbar.
    pub controls: bool,
    /// True once the Controls sheet has been dismissed at least once on
    /// this machine (#834). While false — a true first run — the sheet
    /// is center-anchored so a brand-new visitor cannot miss it; ever
    /// after it is a normal draggable window near the right edge.
    pub controls_seen: bool,
    /// True once the owner-gestures callout has fired (#851): the first
    /// `InGame` arrival in a world the player OWNS re-opens the Controls
    /// sheet so its "You own this world" section (right-click menu,
    /// Shift-copy, Esc) is actually seen. Persisted like the rest of the
    /// struct so it happens once per machine, not once per session.
    pub owner_hint_seen: bool,
}

impl Default for UiPanels {
    fn default() -> Self {
        Self {
            chat: false,
            people: false,
            avatar: false,
            world_editor: false,
            inventory: false,
            catalogue: false,
            diagnostics: false,
            settings: false,
            controls: true,
            controls_seen: false,
            owner_hint_seen: false,
        }
    }
}

/// Does the signed-in player own the overland they're standing in?
/// Ownership is DID equality — the room record lives in the owner's PDS.
pub(crate) fn owns_current_room(
    session: Option<&AtprotoSession>,
    current_room: Option<&CurrentRoomDid>,
) -> bool {
    match (session, current_room) {
        (Some(session), Some(room)) => session.did == room.0,
        _ => false,
    }
}

/// "May the room gizmo, its highlight and its overlays exist right now?"
/// as one system param (#1237 f142).
///
/// Three surfaces used to answer it with three different subsets of the
/// same three facts, and the visitor case fell through all of them: a
/// `RoomEditorState` selection survives portal travel (`TravelingTo` never
/// leaves `InGame`, and the record swap does not touch the editor state),
/// `panels.world_editor` stays `true` for a visitor because the toolbar
/// renders a DISABLED button without clearing the flag and `room_admin_ui`
/// returns at its ownership gate before the reconcile that would, and the
/// placement visualiser gated on neither. The result was a glowing green
/// circle over a stranger's terrain, and a gizmo that could still drag and
/// commit into their room's live record.
///
/// Bundling `panels` in keeps `sync_gizmo_selection` at its existing
/// sixteen parameters rather than seventeen.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RoomEditAccess<'w> {
    pub panels: Res<'w, UiPanels>,
    session: Option<Res<'w, AtprotoSession>>,
    current_room: Option<Res<'w, CurrentRoomDid>>,
}

impl RoomEditAccess<'_> {
    /// The World Editor window is open AND this is the owner's own room.
    /// The exact gate the editor window itself renders under, and the one
    /// `pick_on_scene_click` uses to decide whether a click may select.
    pub fn can_edit_room(&self) -> bool {
        self.panels.world_editor
            && owns_current_room(self.session.as_deref(), self.current_room.as_deref())
    }
}

/// Reserved width of the toolbar wordmark (#860) — fixed so the brand
/// text can never shift the toggles after it (same contract as the
/// badge widths below).
const WORDMARK_WIDTH: f32 = 148.0;

/// Reserved width of the Chat toggle — wide enough for "Chat (99+)" so
/// the unread badge appearing/growing never shifts the buttons after it.
const CHAT_TOGGLE_WIDTH: f32 = 84.0;
/// Reserved width of the People toggle, sized for "People (99+)".
const PEOPLE_TOGGLE_WIDTH: f32 = 100.0;
/// Reserved slot width of the anomaly dot AND its count, occupied even
/// while healthy so the dot appearing/vanishing stops shifting the
/// Controls button. Sized for the widest count `badge_count` prints.
const ANOMALY_DOT_WIDTH: f32 = 40.0;
/// Reserved width of the connection chip (#1213), sized for its widest
/// label ("Connecting…") plus the state dot — same contract as the badges
/// above, so a link that flaps never shifts the account chip beside it.
const LINK_CHIP_WIDTH: f32 = 96.0;

/// Counts above this render as "99+" — the badge is a "look here"
/// signal, not a metric, and capping it keeps the reserved width honest.
const BADGE_COUNT_CAP: usize = 99;

/// A panel toggle with a fixed minimum width and a one-line tooltip:
/// the width reservation is what keeps count-badged labels ("Chat (3)")
/// from shifting the rest of the row as the count changes. Returns
/// whether the flag flipped, for the caller's guarded-dirty bookkeeping.
fn toggle_with_badge(
    ui: &mut egui::Ui,
    flag: &mut bool,
    label: String,
    min_width: f32,
    tip: &str,
) -> bool {
    let size = egui::vec2(min_width, ui.spacing().interact_size.y);
    if ui
        .add_sized(size, egui::Button::selectable(*flag, label))
        .on_hover_text(tip)
        .clicked()
    {
        *flag = !*flag;
        return true;
    }
    false
}

/// Format a badge count, capped so the label can't outgrow its
/// reserved width.
fn badge_count(n: usize) -> String {
    if n > BADGE_COUNT_CAP {
        format!("{BADGE_COUNT_CAP}+")
    } else {
        n.to_string()
    }
}

/// Pick the singular or plural noun for a count (#1264 f374).
///
/// The app already branches on the singular nearly everywhere — the
/// People window's pending offers, this file's anomaly badge, the
/// Inventory header, the audio panel's per-noun suffixes — which is
/// exactly what made "1 entries" in the catalogue, "Downloaded 1 events"
/// and "· 1 props" read as unfinished rather than as a house style. The
/// three stragglers now go through here.
///
/// English-only, and that is fine: there is no i18n framework in the tree
/// (no fluent, gettext or rust-i18n dependency), so a helper is not a
/// translation layer — it is the seam to route through if one is ever
/// added, which is worth more than three inline `if n == 1` branches.
pub(crate) fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// Longest `@handle` the account chip prints before it elides (#1261
/// f235). ATProto handles are domains and a custom one is unbounded —
/// `@someone.a-very-long-custom-domain.example` is a legal handle, and
/// the chip drew it in full at whatever width it came to.
const HANDLE_CHIP_MAX_CHARS: usize = 22;

/// The account chip's label: `@handle`, elided at its first label when
/// the whole thing is too long for a toolbar to promise (#1261 f235).
///
/// Pure so the width measurement and the render read the same string —
/// [`trailing_needed`] measures exactly what gets drawn. The full handle
/// stays in the chip's hover text and in the menu it opens, so nothing
/// is lost, only deferred.
fn account_chip_label(handle: &str) -> String {
    let full = format!("@{handle}");
    if full.chars().count() <= HANDLE_CHIP_MAX_CHARS {
        return full;
    }
    // The first label is the part that identifies a person; the rest is
    // the domain they happen to be hosted under.
    let head = handle.split('.').next().unwrap_or(handle);
    let head: String = head.chars().take(HANDLE_CHIP_MAX_CHARS - 2).collect();
    format!("@{head}…")
}

/// Width a text button will occupy, MEASURED — the galley plus the
/// spacing egui adds around it (#1261 f235).
///
/// Measured and not tabulated, because the answer moves with the font,
/// the `Small`/`Body` sizes and the #1259 f239 zoom factor, and a
/// tabulated constant would be a prediction that goes stale exactly the
/// way #1280's footer reserve did.
fn button_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .ctx()
        .fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER));
    galley.size().x + 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x
}

/// The four controls that fold into the `…` menu when the bar runs out
/// of room, in the order they are drawn right-to-left. Named once so the
/// measurement and the render cannot drift —
/// `every_measured_label_is_a_label_this_file_draws` is what holds them
/// together.
const TRAILING_LABELS: [&str; 4] = ["🔊 Mute", "Diagnostics", "Settings", "Controls"];

/// The left group's variable-width toggles. Measurement only: the render
/// spells them out because each carries its own hover text and the World
/// Editor one has an ownership branch. The Chat, People and wordmark
/// slots are not here — they have reserved-width constants of their own.
#[cfg(test)]
const LEADING_LABELS: [&str; 4] = ["Avatar", "Inventory", "Catalogue", "World Editor"];

/// Width the trailing group needs to draw everything unfolded.
///
/// The connection chip, the account chip and the anomaly slot are NOT in
/// here: they never fold. A link state, who you are signed in as, and a
/// session that has gone wrong are the three things a bar this size must
/// keep saying.
fn trailing_needed(ui: &egui::Ui, account_chip: &str) -> f32 {
    LINK_CHIP_WIDTH
        + ANOMALY_DOT_WIDTH
        + button_width(ui, account_chip)
        + TRAILING_LABELS
            .iter()
            .map(|label| button_width(ui, label))
            .sum::<f32>()
}

/// How the reserved anomaly slot senses input (#1260 f248).
///
/// `Sense::click()` is `interactive()`, and `interactive()` is what makes
/// an allocated rect Tab-reachable. The slot used to take one
/// unconditionally, so on a healthy session — the common case, when
/// nothing is painted there — keyboard focus landed on a blank gap
/// between Diagnostics and Settings that showed nothing, said nothing
/// and did nothing on Enter. `Sense::hover()` keeps the reservation (the
/// dot appearing must not shift the Controls button) and drops the tab
/// stop.
fn anomaly_slot_sense(has_anomaly: bool) -> egui::Sense {
    if has_anomaly {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    }
}

/// Slim top bar enumerating every panel as a toggle button. The World
/// Editor button is enabled only for the room's owner — the panel
/// itself is owner-gated too. Visitors see it disabled with an
/// ownership explanation instead of not at all (#851).
///
/// #835 additions: one-line tooltips on every toggle, an unread-count
/// badge on Chat, a live headcount on People, a clickable anomaly dot
/// that opens Diagnostics on the worst tab, and an account chip at the
/// far right (identity, current room, Copy Landmark Link, Log out — the
/// two-click home for actions that used to hide in Diagnostics→Identity).
/// Everything the account chip reads, bundled.
///
/// `toolbar_ui` was at Bevy's 16-parameter `IntoSystem` ceiling exactly —
/// an over-ceiling system fails at app build with a trait error naming
/// none of this — and #1232 f251's "Travel to my world" needs two more
/// (`TravelingTo` and `UnsavedGuard`) to disable itself with a reason. So
/// the chip's own six move into a struct first, the way `people_ui` got
/// `RosterDeps` (#1223 f291).
#[derive(SystemParam)]
pub struct AccountChip<'w, 's> {
    session: Option<Res<'w, AtprotoSession>>,
    current_room: Option<Res<'w, CurrentRoomDid>>,
    profile_cache: Res<'w, BskyProfileCache>,
    local_player: Query<'w, 's, &'static Transform, With<LocalPlayer>>,
    /// Copy Landmark Link reports through the clipboard queue (#1141),
    /// which is also where its toast is raised.
    clipboard: Res<'w, crate::boot_params::ClipboardQueue>,
    /// #1214: the second door onto the re-authenticate flow, for an owner
    /// who dismissed the modal. The account menu is where every other
    /// session control already lives.
    expired: Option<ResMut<'w, crate::ui::reauth::SessionExpired>>,
    /// #1232 f251: a travel already in flight, and a guard dialog already
    /// open, are the two states the home row must refuse to stack behind.
    traveling: Option<Res<'w, crate::state::TravelingTo>>,
    guard: Option<Res<'w, crate::ui::unsaved_guard::UnsavedGuard>>,
    /// #1240 f159: the unstuck command. The menu is where it belongs —
    /// somebody wedged between a settlement wall and a rock can still
    /// reach the toolbar, and there is no free key that does not collide
    /// with movement.
    return_to_spawn: ResMut<'w, crate::player::PlayerMoveRequest>,
    /// Whether there is any ground to be returned TO.
    terrain: Option<Res<'w, crate::terrain::FinishedHeightMap>>,
}

#[allow(clippy::too_many_arguments)]
pub fn toolbar_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    mut audio_muted: ResMut<crate::audio_mute::AudioMuted>,
    invariants: Res<InvariantRegistry>,
    mut chat: ResMut<ChatHistory>,
    peers: Query<&RemotePeer>,
    mut diag_tab: ResMut<crate::ui::diagnostics::DiagTab>,
    mut commands: Commands,
    mut panel_free: ResMut<crate::ui::layout::PanelFreeRect>,
    // #1213: the client's own answer to "am I connected?". Before this the
    // toolbar's "People (1)" was the closest thing to a connection surface,
    // and it said the same thing during an outage as in an empty room.
    link: Res<crate::network::LinkState>,
    mut chip: AccountChip,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let owns_room = owns_current_room(chip.session.as_deref(), chip.current_room.as_deref());

    // Guarded-dirty (#879): `&mut panels.x` through the `ResMut` marks
    // UiPanels changed EVERY frame the toolbar draws — which re-armed
    // the prefs save debounce forever, so preferences only ever hit disk
    // at logout (the first frame the toolbar stops drawing). Borrow
    // bypassed and tick `set_changed` only on a real toggle, the same
    // idiom as the Settings window.
    let p = panels.bypass_change_detection();
    let mut panels_dirty = false;

    // An open Chat window means every message is on screen — the badge
    // only counts what arrives while it's closed. Guarded write so the
    // resource isn't marked changed every frame the window sits open.
    if p.chat && chat.unread != 0 {
        chat.unread = 0;
    }
    let chat_label = if chat.unread > 0 {
        format!("Chat ({})", badge_count(chat.unread))
    } else {
        "Chat".to_owned()
    };
    // Everyone in the room, self included — matching the People window's
    // own "In room (N)" header.
    let people_total = peers.iter().count() + chip.session.is_some() as usize;

    // egui 0.35 shows panels into a `Ui`, not a `Context`: a top-level
    // panel draws into a screen-sized background layer (bevy_egui 0.41's
    // side_panel example). What the panel leaves of that layer is the rect
    // every window constrains to, published below as `PanelFreeRect`.
    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        "overlands-viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::top("overlands-toolbar").show(&mut viewport_ui, |ui| {
        ui.horizontal(|ui| {
            // Wordmark (#860): the product's name at the bar's left edge.
            // Non-interactive, accent-coloured, fixed width — the brand
            // anchor for every session, and the same product identity the
            // wasm splash and OAuth return pages carry.
            let accent = crate::ui::theme::current(ui.ctx()).accent;
            ui.add_sized(
                egui::vec2(WORDMARK_WIDTH, ui.spacing().interact_size.y),
                egui::Label::new(
                    egui::RichText::new("SYMBIOS OVERLANDS")
                        .strong()
                        .color(accent),
                )
                .selectable(false),
            );
            ui.separator();
            panels_dirty |= toggle_with_badge(
                ui,
                &mut p.chat,
                chat_label,
                CHAT_TOGGLE_WIDTH,
                "Chat — talk with everyone in this world (Enter)",
            );
            panels_dirty |= toggle_with_badge(
                ui,
                &mut p.people,
                format!("People ({})", badge_count(people_total)),
                PEOPLE_TOGGLE_WIDTH,
                "People — who's here; drag an item onto a row to gift it",
            );
            panels_dirty |= ui
                .toggle_value(&mut p.avatar, "Avatar")
                .on_hover_text("Avatar — edit your look and vehicle")
                .changed();
            panels_dirty |= ui
                .toggle_value(&mut p.inventory, "Inventory")
                .on_hover_text("Inventory — the items you own")
                .changed();
            panels_dirty |= ui
                .toggle_value(&mut p.catalogue, "Catalogue")
                .on_hover_text("Catalogue — browse placeable items")
                .changed();
            if owns_room {
                panels_dirty |= ui
                    .toggle_value(&mut p.world_editor, "World Editor")
                    .on_hover_text("World Editor — reshape this world (you own it)")
                    .changed();
            } else {
                // Rendered disabled instead of hidden (#851): the silent
                // pop-in taught nobody why the button exists — now a
                // visitor hovering it learns the ownership rule.
                ui.add_enabled(false, egui::Button::selectable(false, "World Editor"))
                    .on_disabled_hover_text(
                        "Only this overland's owner can edit it. Your own overland \
                         is editable when you're home.",
                    );
            }
            // #1261 f235: the bar is one non-wrapping row with no overflow
            // policy at all. At 1280x720 it fits; at a 1024-CSS-px browser
            // window, at 125% OS scaling, or after two presses of the
            // #1259 f239 zoom, the TAIL of the right-to-left group runs
            // leftward past its own rect and overpaints the toggles there.
            // The refuter corrected the direction the finding claimed: in
            // `right_to_left` items are placed from the right edge in ADD
            // order, so the account chip — added first — is the most
            // protected item, and Controls and Settings, added last, are
            // the first to collide.
            //
            // The room is MEASURED, not thresholded: `trailing_needed`
            // lays out the real labels at the live font and zoom, so the
            // policy fires when the bar actually runs out of room rather
            // than at a pixel count somebody guessed once.
            let account_chip = chip
                .session
                .as_deref()
                .map(|sess| account_chip_label(&sess.handle))
                .unwrap_or_default();
            let trailing_overflows = trailing_needed(ui, &account_chip) > ui.available_width();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Connection chip (#1213). Drawn FIRST in the right-to-left
                // layout so it owns the far-right corner, ahead of the
                // account chip: whether the client is connected outranks
                // who it is connected as. Dot AND word — the state has to
                // survive a greyscale viewer — in a reserved-width slot so
                // "Connecting…" cannot shove the row about.
                link_chip(ui, link.phase());
                // Account chip — next in the right-to-left layout. The only
                // 2-click route to logout and location sharing (#835);
                // Diagnostics keeps its duplicates.
                if let Some(sess) = chip.session.as_deref() {
                    ui.menu_button(account_chip_label(&sess.handle), |ui| {
                        ui.horizontal(|ui| {
                            crate::avatar::draw_avatar_icon(
                                ui,
                                Some(sess.did.as_str()),
                                Some(sess.handle.as_str()),
                                &chip.profile_cache,
                                crate::ui::chat::AVATAR_ICON_PX,
                            );
                            ui.monospace(format!("@{}", sess.handle));
                        });
                        ui.monospace(
                            egui::RichText::new(&sess.did)
                                .small()
                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        );
                        if let Some(room) = chip.current_room.as_deref() {
                            ui.separator();
                            ui.label(if owns_room {
                                "Current world: yours"
                            } else {
                                "Current world:"
                            });
                            if !owns_room {
                                ui.monospace(
                                    egui::RichText::new(&room.0)
                                        .small()
                                        .color(crate::ui::theme::current(ui.ctx()).text_weak),
                                );
                            }
                            let player_tf = chip.local_player.single().ok().copied();
                            if crate::ui::diagnostics::landmark_link_button(
                                ui,
                                &room.0,
                                player_tf,
                                &chip.clipboard,
                            ) {
                                ui.close();
                            }
                        }
                        ui.separator();
                        // The unstuck command (#1240 f159). Before this the
                        // only recovery in the whole app was
                        // `respawn_if_fallen`, which fires 20 m BELOW local
                        // ground — so geometry that traps you ABOVE the
                        // terrain (a construct collider, a crevasse, a
                        // settlement wall, a pit a skiff cannot climb out
                        // of) never satisfied it and the only exit was
                        // logging out.
                        let stuck_blocked = crate::player::return_to_spawn_blocked(
                            chip.terrain.is_some(),
                            chip.traveling.is_some(),
                        );
                        let unstuck = ui
                            .add_enabled(
                                stuck_blocked.is_none(),
                                egui::Button::new("Return to spawn"),
                            )
                            .on_hover_text(
                                "Puts you back on solid ground in this overland — \
                                 for when you are wedged and cannot move",
                            );
                        let unstuck = match stuck_blocked {
                            Some(reason) => unstuck.on_disabled_hover_text(reason),
                            None => unstuck,
                        };
                        if unstuck.clicked() {
                            chip.return_to_spawn
                                .request(crate::player::PlayerMove::ReturnToSpawn);
                            ui.close();
                        }
                        // The route home (#1232 f251). Until this existed
                        // the only one was the gateway picker's home row,
                        // inside a window that opens solely while standing
                        // in the host's gate — and a landmark link can put
                        // the arrival anywhere, with nothing pointing at
                        // the gate. The remaining exit was Log out, which
                        // is the action the app guards as destructive.
                        //
                        // Same guard flow, same `target_pos: None`, as the
                        // gateway home row and the People *Visit* button.
                        let home_blocked = crate::ui::travel::home_travel_blocked(
                            owns_room,
                            chip.traveling.is_some(),
                            chip.guard.is_some(),
                        );
                        let go_home = ui
                            .add_enabled(
                                home_blocked.is_none(),
                                egui::Button::new("Travel to my world"),
                            )
                            .on_hover_text("Go back to your own world");
                        let go_home = match home_blocked {
                            Some(reason) => go_home.on_disabled_hover_text(reason),
                            None => go_home,
                        };
                        if go_home.clicked() {
                            commands.insert_resource(UnsavedGuard::new(
                                GuardedAction::PortalTravel {
                                    via: crate::ui::unsaved_guard::TravelVia::Menu,
                                    target_did: sess.did.clone(),
                                    target_label: Some(format!("@{}", sess.handle)),
                                    target_pos: None,
                                },
                            ));
                            ui.close();
                        }
                        // Above Log out deliberately: with the session
                        // expired, logging out is the door that discards
                        // the work and this is the one that keeps it.
                        if let Some(expired) = chip.expired.as_deref_mut()
                            && ui
                                .button("Sign in again")
                                .on_hover_text(
                                    "Your session has expired — sign in again to \
                                     save. Your unsaved edits stay as they are.",
                                )
                                .clicked()
                        {
                            expired.dismissed = false;
                            ui.close();
                        }
                        if ui.button("Log out").clicked() {
                            // Route through the unsaved-edits guard instead
                            // of flipping the state directly: it transitions
                            // immediately when nothing is dirty, and offers
                            // Publish / Discard / Cancel otherwise.
                            commands.insert_resource(UnsavedGuard::new(GuardedAction::Logout));
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text(format!(
                        "@{} — identity, share your spot, log out",
                        sess.handle
                    ));
                }
                // The four collapsible controls, folded into a `…` menu
                // when the bar cannot hold them (#1261 f235). Folded
                // rather than clipped: Log out has exactly one route (the
                // account chip's menu) and the theme picker a low-vision
                // user needs has exactly one (Settings), so the failure
                // mode this replaces was losing both with no error and no
                // keyboard alternative.
                //
                // Drawn in a closure so the bar and the menu are the same
                // code — a second copy is how the two would drift.
                let mut trailing =
                    |ui: &mut egui::Ui, p: &mut UiPanels, panels_dirty: &mut bool| {
                        // Master mute, as a labelled toggle rather than a bare
                        // emoji (#1260 f240). The action word used to live only
                        // in the hover, which egui opens for a pointer and never
                        // for keyboard focus — so tabbing onto it gave a
                        // keyboard-only user a glyph and nothing else. A
                        // `toggle_value` also matches the People roster's "Mute"
                        // checkbox, so the same word means the same thing in
                        // both places, and its checked state is a second cue.
                        //
                        // Guarded-dirty (#879): `&mut audio_muted.0` through the
                        // `ResMut` would mark the resource changed every frame.
                        let mut muted = audio_muted.0;
                        let label = format!("{} Mute", if muted { "🔇" } else { "🔊" });
                        let mute_resp = ui.toggle_value(&mut muted, label);
                        if crate::ui::affordances::hint(
                            mute_resp,
                            if muted {
                                "All audio is muted. Click to hear the world again."
                            } else {
                                "Silence all audio — the world, other people and effects."
                            },
                        )
                        .changed()
                        {
                            audio_muted.0 = muted;
                        }
                        *panels_dirty |= ui
                            .toggle_value(&mut p.diagnostics, "Diagnostics")
                            .on_hover_text("Diagnostics — session health, metrics, and logs")
                            .changed();
                    };
                if !trailing_overflows {
                    trailing(ui, p, &mut panels_dirty);
                }
                // Worst-active anomaly dot (D-6): a severity-coloured ●
                // beside the Diagnostics toggle whenever an invariant is
                // violated, so a broken session is visible even with the
                // panel closed. The slot is reserved even while healthy so
                // the dot's appearance doesn't shift the Controls button;
                // clicking it opens Diagnostics on the worst tab (#835).
                //
                // The slot senses a CLICK only while there is something to
                // click (#1260 f248). It used to sense one unconditionally,
                // and `Sense::click()` is `interactive()`, which is what
                // makes a rect Tab-reachable — so on a healthy session,
                // the common case, keyboard focus landed on a 14-point gap
                // that painted nothing, said nothing and did nothing on
                // Enter. `Sense::hover()` is not interactive, so the
                // reservation stays and the tab stop goes.
                let worst = invariants.worst_active();
                let slot = egui::vec2(ANOMALY_DOT_WIDTH, ui.spacing().interact_size.y);
                let (dot_rect, dot_resp) =
                    ui.allocate_exact_size(slot, anomaly_slot_sense(worst.is_some()));
                if let Some(worst) = worst {
                    // Painted circle, not a "●" glyph — U+25CF is
                    // tofu in the proportional family (#861).
                    let colour = crate::ui::diagnostics::severity_color(ui, worst);
                    let n = invariants.active_badges().count();
                    ui.painter().circle_filled(
                        dot_rect.left_center() + egui::vec2(6.0, 0.0),
                        4.5,
                        colour,
                    );
                    // The COUNT beside the dot, not only in the hover
                    // (#1260 f240): a painted circle carries no text in
                    // any input mode, and the tooltip that explained it
                    // opens for a pointer and never for keyboard focus.
                    // Drawn inside the reserved slot, so nothing shifts.
                    ui.painter().text(
                        dot_rect.left_center() + egui::vec2(14.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        badge_count(n),
                        egui::TextStyle::Body.resolve(ui.style()),
                        colour,
                    );
                    let dot_resp = crate::ui::affordances::hint(
                        dot_resp.on_hover_cursor(egui::CursorIcon::PointingHand),
                        &format!(
                            "{n} active anomal{} — click to open Diagnostics",
                            if n == 1 { "y" } else { "ies" }
                        ),
                    );
                    if dot_resp.clicked() {
                        panels_dirty |= !p.diagnostics;
                        p.diagnostics = true;
                        *diag_tab = crate::ui::diagnostics::tab_for_subsystem(
                            invariants.worst_active_subsystem(),
                        );
                    }
                }
                // Added AFTER the dot in this right-to-left layout so the
                // dot stays glued to the Diagnostics toggle it belongs to
                // (visual order: … Controls · Settings · ● · Diagnostics).
                let tail = |ui: &mut egui::Ui, p: &mut UiPanels, panels_dirty: &mut bool| {
                    *panels_dirty |= ui
                        .toggle_value(&mut p.settings, "Settings")
                        .on_hover_text("Settings — theme & client preferences")
                        .changed();
                    *panels_dirty |= ui
                        .toggle_value(&mut p.controls, "Controls")
                        .on_hover_text("Controls — movement & camera cheat-sheet")
                        .changed();
                };
                if trailing_overflows {
                    // One `…` in place of the four, holding them in the
                    // order they would have appeared left-to-right on a
                    // bar with room. The anomaly dot above stays on the
                    // bar whatever happens: a session that has gone wrong
                    // must not be able to hide inside a menu.
                    ui.menu_button("…", |ui| {
                        tail(ui, p, &mut panels_dirty);
                        trailing(ui, p, &mut panels_dirty);
                    })
                    .response
                    .on_hover_text("More — Controls, Settings, Diagnostics and Mute");
                } else {
                    tail(ui, p, &mut panels_dirty);
                }
            });
        });
    });
    panel_free.0 = Some(viewport_ui.available_rect_before_wrap());

    if panels_dirty {
        panels.set_changed();
    }
}

/// The connection chip: a state dot and the phase's own word, in a
/// reserved-width slot (#1213).
///
/// The app had no connection surface at all before this — a dead socket
/// looked exactly like an empty world, which is the same thing a product
/// with no users looks like. The dot is a painted circle rather than a "●"
/// glyph for the reason the anomaly dot gives (U+25CF is tofu in the
/// proportional family, #861), and the word is always drawn beside it so
/// the state is never carried by hue alone.
fn link_chip(ui: &mut egui::Ui, phase: crate::network::LinkPhase) {
    let th = crate::ui::theme::current(ui.ctx());
    let (colour, text_colour) = match phase {
        crate::network::LinkPhase::Connected => (th.status.ok, th.text_weak),
        crate::network::LinkPhase::Connecting => (th.status.warn, th.status.warn),
        crate::network::LinkPhase::Down => (th.status.error, th.status.error),
    };
    let slot = egui::vec2(LINK_CHIP_WIDTH, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(slot, egui::Sense::hover());
    let painter = ui.painter();
    // Right-to-left layout: the slot's own contents read left-to-right, so
    // the dot sits at the slot's left edge and the label follows it.
    let dot_x = rect.left() + 7.0;
    painter.circle_filled(egui::pos2(dot_x, rect.center().y), 4.5, colour);
    painter.text(
        egui::pos2(dot_x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        phase.chip_label(),
        egui::TextStyle::Body.resolve(ui.style()),
        text_colour,
    );
    response.on_hover_text(phase.sentence());
}

/// The chassis the local player is currently piloting, resolved from the
/// preset marker on the [`LocalPlayer`] entity. Drives which movement key rows
/// the Controls cheat-sheet shows (#803).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PilotedChassis {
    OnFoot,
    Boat,
    Skiff,
    Airship,
    Airplane,
    /// A spawned body carrying NO preset marker (#1241 f161): the record
    /// names a locomotion preset this build does not model, so
    /// `build_preset_components` inserted a bare collider and nothing
    /// else, and not one drive system runs. The sheet used to fall back to
    /// `OnFoot` here and cheerfully list the walk keys for an avatar that
    /// could not move at all.
    Unrecognised,
}

impl PilotedChassis {
    /// Player-facing name for the sheet's "Piloting:" heading (#834) —
    /// the rows already swap live with the chassis (#803), but without
    /// this the window never said WHICH chassis they describe.
    ///
    /// **Read from the preset's own `DISPLAY_LABEL`, not spelled again
    /// here** (#1266 f167). This sheet's footer sends the reader one click
    /// away to the Locomotion picker; the two lists used to agree on
    /// Airplane and nothing else, so choosing "Car" and opening Controls
    /// said you were piloting a "Skiff" — a vehicle the picker does not
    /// offer. There is no way for a user to resolve a rename they can see
    /// both halves of, so the string exists once.
    ///
    /// `Unrecognised` has no preset by definition, so its words are its
    /// own.
    fn label(self) -> &'static str {
        use crate::pds::avatar::locomotion::{
            AirplaneParams, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams,
            LocomotionPreset,
        };
        match self {
            Self::OnFoot => HumanoidParams::DISPLAY_LABEL,
            Self::Boat => HoverBoatParams::DISPLAY_LABEL,
            Self::Skiff => CarParams::DISPLAY_LABEL,
            Self::Airship => HelicopterParams::DISPLAY_LABEL,
            Self::Airplane => AirplaneParams::DISPLAY_LABEL,
            Self::Unrecognised => "Unknown vehicle",
        }
    }
}

/// One key-binding row in the Controls cheat-sheet: the key glyphs and what
/// they do on the current chassis.
struct ControlRow {
    keys: &'static str,
    action: &'static str,
}

// Per-chassis movement rows. These mirror the live key handlers in
// `player/{humanoid,hover_boat,car,helicopter,airplane}.rs`, so the sheet can
// never drift from the actual controls again (#803) — change both together.
//
// It drifted anyway, because nothing tested it: the #803 guards pinned
// GLOBAL_ROWS and EDITOR_ROWS only. `on_foot_rows_mirror_the_humanoid_
// handler` closes that (#1235 f40/f41) — Shift became the run key with
// #1193 and the sheet went on calling it "swim down", while Space
// advertised a "climb" the humanoid controller has never implemented.
const ON_FOOT_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W A S D  or  Arrows",
        action: "walk",
    },
    ControlRow {
        // #1193: the record's `walk_speed` IS the run, and unshifted
        // movement walks at the body's own derived pace.
        keys: "Shift",
        action: "run (on land)",
    },
    ControlRow {
        keys: "Space",
        action: "jump · swim up",
    },
    ControlRow {
        // Ctrl is not bound on wasm (#839): W+Ctrl is the browser's
        // close-tab chord. Mirrors `player::humanoid`'s swim keys.
        keys: if cfg!(target_arch = "wasm32") {
            "Shift / C"
        } else {
            "Shift / Ctrl / C"
        },
        action: "swim down (in water — Shift stops meaning run)",
    },
];
const BOAT_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W / S  or  ⬆ / ⬇",
        action: "drive forward / reverse",
    },
    ControlRow {
        keys: "A / D  or  ⬅ / ➡",
        action: "steer",
    },
    ControlRow {
        keys: "Space",
        action: "hop up",
    },
];
const SKIFF_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W / S  or  ⬆ / ⬇",
        action: "throttle / reverse",
    },
    ControlRow {
        keys: "A / D  or  ⬅ / ➡",
        action: "steer (on the ground)",
    },
    ControlRow {
        keys: "Space",
        action: "handbrake",
    },
];
const AIRSHIP_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W / S  or  ⬆ / ⬇",
        action: "fly forward / back",
    },
    ControlRow {
        keys: "A / D  or  ⬅ / ➡",
        action: "yaw (turn)",
    },
    ControlRow {
        keys: "Q / E",
        action: "strafe left / right",
    },
    ControlRow {
        keys: "Space / Shift",
        action: "climb / descend",
    },
];
// The one "row" for a preset this build cannot drive (#1241 f161). Not a
// key binding: it is the sentence that replaces the key bindings, and it
// points at the ONE surface that can fix it — which used to be reachable
// only by a warn-coloured paragraph two clicks into Avatar › Locomotion,
// which nobody has a reason to open.
const UNRECOGNISED_ROWS: &[ControlRow] = &[ControlRow {
    keys: "—",
    action: "this build can't drive your preset · pick one in Avatar › Locomotion",
}];
const AIRPLANE_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W / S  or  ⬆ / ⬇",
        action: "pitch down / up",
    },
    ControlRow {
        keys: "A / D  or  ⬅ / ➡",
        action: "roll",
    },
    ControlRow {
        keys: "Q / E",
        action: "yaw (rudder)",
    },
    ControlRow {
        keys: "Space / Shift",
        action: "throttle up / down",
    },
];

// World-editing gesture rows (#851), shown to the room's owner. These
// mirror the live handlers — same #803 contract as the movement rows,
// change both together:
// * right-click menu → `editor_gizmo::context_menu::detect_scene_right_click`
//   (click-vs-drag discrimination: a right-DRAG still orbits the camera)
// * left-click pick  → the editor's scene picker (active while an editor
//   window is open)
// * Shift-copy-drag  → `editor_gizmo::drag` (Shift at drag-start clones)
// * Esc              → drag abort + selection clear (`ui::shortcuts`)
// Right-click rows every visitor can perform (#1235 f149). The scene
// menu's avatar entries are explicitly NOT owner-gated — "those work for
// visitors too" (`editor_gizmo::context_menu`) — yet the only place in the
// app documenting right-click at all was the owner-gated block below, so a
// visitor never learned that Take off / Re-seat / Save to inventory /
// Wear from inventory exist. Right-click doubling as camera orbit actively
// teaches people not to try it.
const AVATAR_ROWS: &[ControlRow] = &[ControlRow {
    keys: "Right-click yourself",
    action: "your body or a worn item: edit · re-seat · take off · wear",
}];

const EDITOR_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "Right-click",
        action: "create / select menu (a right-DRAG still orbits)",
    },
    ControlRow {
        keys: "Left-click",
        action: "pick the object under the cursor (editor open)",
    },
    ControlRow {
        keys: "Shift-drag",
        action: "drag a copy instead of moving",
    },
    ControlRow {
        keys: "Esc",
        action: "abort a drag · clear the selection",
    },
    // #1244 f148: a tree-row click attaches the gizmo to whichever live
    // instance is nearest the camera, which can be far away or behind
    // you — half the time selecting from the tree showed nothing at all.
    ControlRow {
        keys: "F",
        action: "go to the selected object",
    },
];

// Global shortcut rows (#836, #864) — the same on every chassis, and the
// only place any of them is written down. A `const` rather than inline
// grid rows so the sheet's coverage is testable: Ctrl+Z / Ctrl+Shift+Z
// shipped with #864 and stayed discoverable only through the hover text
// of an editor's Undo button (#1141).
//
// Same contract as the movement and editor rows — these mirror live
// handlers, change both together:
// * Enter          → `ui::shortcuts` chat focus
// * Esc            → drag abort · selection clear · window close
// * Ctrl+S         → `ui::shortcuts` publish
// * Ctrl+Z / Shift → `ui::undo::trigger`
// Camera rows — the same on every chassis. A const rather than inline
// grid rows since #1235 f166: the pan row advertised a middle button a
// laptop does not have, with no alternative anywhere, and a cheat-sheet
// row that cannot be performed is worse than no row.
const CAMERA_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "Right-drag",
        action: "orbit camera",
    },
    ControlRow {
        keys: "Middle-drag  or  Alt + right-drag",
        action: "pan camera",
    },
    ControlRow {
        keys: "Scroll  or  pinch",
        action: "zoom",
    },
];

const GLOBAL_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "Enter",
        action: "open chat",
    },
    ControlRow {
        keys: "Esc",
        action: "back out one step: drag · selection · pop-out · gateway · windows",
    },
    ControlRow {
        keys: "Ctrl+S",
        action: "save the front-most editor (opens it if none is open)",
    },
    ControlRow {
        keys: "Ctrl+Z / Ctrl+Shift+Z",
        action: "undo / redo in the open editor",
    },
    // egui has always bound these (#1259 f239) — `zoom_with_keyboard`
    // defaults on — and until now the app said so nowhere and forgot the
    // result at every launch. `theme::sync_ui_scale` reads the zoom back
    // into the persisted setting, so a user who finds the shortcut keeps
    // what they chose, and Settings shows them the same number.
    ControlRow {
        keys: "Ctrl+plus / Ctrl+minus",
        action: "make the interface bigger / smaller (also in Settings)",
    },
    // Not a key: the unstuck command (#1240 f159) has no binding that does
    // not collide with movement, and the sheet is the only place that can
    // tell anybody it exists. Listed here because being unable to move is
    // the failure a cheat-sheet most needs to answer.
    ControlRow {
        keys: "Your @name menu",
        action: "Return to spawn — if you are wedged and cannot move",
    },
];

/// Movement key rows for the piloted chassis — the pure preset→rows mapping
/// (#803, unit-tested below). The camera rows and portal hint are shared and
/// rendered separately by [`controls_hint_ui`].
fn movement_rows(chassis: PilotedChassis) -> &'static [ControlRow] {
    match chassis {
        PilotedChassis::OnFoot => ON_FOOT_ROWS,
        PilotedChassis::Boat => BOAT_ROWS,
        PilotedChassis::Skiff => SKIFF_ROWS,
        PilotedChassis::Airship => AIRSHIP_ROWS,
        PilotedChassis::Airplane => AIRPLANE_ROWS,
        PilotedChassis::Unrecognised => UNRECOGNISED_ROWS,
    }
}

/// Resolve the piloted chassis from the `LocalPlayer`'s preset markers (only
/// one is ever present — the hot-swap strips the old before inserting the new).
///
/// A body with NO marker at all is [`PilotedChassis::Unrecognised`]
/// (#1241 f161), not `OnFoot`: `build_preset_components` gives an unknown
/// preset a bare collider and no marker, and every drive system is
/// marker-queried, so that body is an inert falling cube. It used to
/// resolve to `OnFoot` and the sheet listed the walk keys under
/// "Piloting: On foot" — total immobility presented as normal movement.
///
/// The caller distinguishes "no marker" from "no body yet": the query
/// returns nothing at all before the local player spawns, and that case
/// still shows the on-foot default.
fn piloted_chassis(
    boat: bool,
    skiff: bool,
    airship: bool,
    airplane: bool,
    humanoid: bool,
) -> PilotedChassis {
    if boat {
        PilotedChassis::Boat
    } else if skiff {
        PilotedChassis::Skiff
    } else if airship {
        PilotedChassis::Airship
    } else if airplane {
        PilotedChassis::Airplane
    } else if humanoid {
        PilotedChassis::OnFoot
    } else {
        PilotedChassis::Unrecognised
    }
}

/// First `InGame` arrival in a world the player OWNS re-opens the
/// Controls sheet so the owner-gestures section is actually seen once
/// (#851). Latched via the persisted [`UiPanels::owner_hint_seen`], so
/// it fires once per machine — visiting other worlds doesn't count, and
/// re-logins don't re-flash it. Registered on `OnEnter(InGame)`.
pub fn flash_owner_controls_once(
    mut panels: ResMut<UiPanels>,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
) {
    if owns_current_room(session.as_deref(), current_room.as_deref()) && !panels.owner_hint_seen {
        panels.owner_hint_seen = true;
        panels.controls = true;
    }
}

/// Points of the panel-free rect the Controls sheet gives up to its own
/// title bar, frame and margins before its body starts scrolling (#1235
/// f245).
const SHEET_CHROME_SLACK: f32 = 64.0;

/// Floor under that subtraction, so a viewport shorter than the chrome
/// still yields a scrollable body rather than a zero-height one.
const SHEET_MIN_BODY_HEIGHT: f32 = 160.0;

/// Movement / camera cheat-sheet. Open on first `InGame` entry (the
/// [`UiPanels`] default) and from the toolbar afterwards. The movement rows are
/// context-sensitive to the chassis the player is currently piloting (#803);
/// the camera rows and portal hint are shared, and the room's owner
/// additionally gets the world-editing gesture rows (#851).
#[allow(clippy::type_complexity)]
pub fn controls_hint_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    local: Query<
        (
            Has<HoverBoatPreset>,
            Has<CarPreset>,
            Has<HelicopterPreset>,
            Has<AirplanePreset>,
            Has<crate::player::HumanoidPreset>,
        ),
        With<LocalPlayer>,
    >,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
) {
    if !panels.controls {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let chassis = local.iter().next().map_or(
        // No local player entity yet — not the same as a body with no
        // preset marker, which is `Unrecognised` (#1241 f161).
        PilotedChassis::OnFoot,
        |(boat, skiff, airship, airplane, humanoid)| {
            piloted_chassis(boat, skiff, airship, airplane, humanoid)
        },
    );

    let mut open = true;
    let mut window = egui::Window::new("Controls")
        .open(&mut open)
        .collapsible(false)
        .resizable(false);
    // Center-anchored ONLY on a true first run, where missing it would
    // strand a brand-new visitor (#834). `.anchor()` re-pins every
    // frame — permanently immovable — so once the sheet has been seen
    // it becomes a normal draggable window near the right edge, and can
    // no longer superimpose with the (also centered) offer modal.
    let free = chrome.available_rect(ctx);
    if panels.controls_seen {
        let (pos, _size) = chrome.place(crate::ui::layout::UiWindow::Controls, ctx);
        window = window.default_pos(pos).constrain_to(free);
    } else {
        // Constrained on the first run TOO (#1235 f245). The anchored
        // branch had no `constrain_to` at all, and the owner variant — the
        // tallest, and the one `flash_owner_controls_once` opens by itself
        // — runs to roughly 570pt against a 470pt laptop viewport, pushing
        // "Got it" below the fold and the title-bar [x] above it on a
        // window that is `.resizable(false)` and re-pinned every frame.
        window = window
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .constrain_to(free);
    }
    let response = window.show(ctx, |ui| {
        // …and the body scrolls, which is what actually bounds it:
        // `constrain_to` moves and caps the window, but a non-resizable
        // window with unscrolled content still lays out taller than the
        // cap. `SHEET_CHROME_SLACK` leaves the title bar and the window
        // frame their room.
        egui::ScrollArea::vertical()
            .max_height((free.height() - SHEET_CHROME_SLACK).max(SHEET_MIN_BODY_HEIGHT))
            .show(ui, |ui| {
                ui.strong(format!("Piloting: {}", chassis.label()));
                ui.add_space(4.0);
                egui::Grid::new("controls-grid")
                    .num_columns(2)
                    .spacing([24.0, 4.0])
                    .show(ui, |ui| {
                        for row in movement_rows(chassis) {
                            ui.monospace(row.keys);
                            ui.label(row.action);
                            ui.end_row();
                        }
                        // Camera controls are the same on every chassis.
                        for row in CAMERA_ROWS {
                            ui.monospace(row.keys);
                            ui.label(row.action);
                            ui.end_row();
                        }
                        for row in GLOBAL_ROWS {
                            ui.monospace(row.keys);
                            ui.label(row.action);
                            ui.end_row();
                        }
                    });
                ui.add_space(6.0);
                ui.small("Change your vehicle in Avatar › Locomotion.");
                ui.add_space(6.0);
                // The chat-keyword emotes (#1068) had no UI surface at all — a
                // shipped feature nobody could find without typing one of its
                // words by chance (#1141). Sourced from the keyword table so the
                // example words cannot drift from the ones that gesture.
                ui.label(crate::player::emote::Emote::hint_line());
                ui.add_space(6.0);
                ui.label(
                    "Walk through a portal doorway — or a gateway — to travel into \
             another overland.",
                );
                // Visitor-usable right-click, shown to everyone (#1235 f149).
                ui.add_space(6.0);
                egui::Grid::new("controls-avatar-grid")
                    .num_columns(2)
                    .spacing([24.0, 4.0])
                    .show(ui, |ui| {
                        for row in AVATAR_ROWS {
                            ui.monospace(row.keys);
                            ui.label(row.action);
                            ui.end_row();
                        }
                    });
                // Owner-only: the world-editing gestures (#851). Every one of
                // these was previously undiscoverable — and right-click doubling
                // as camera orbit actively taught people to avoid the menu.
                if owns_current_room(session.as_deref(), current_room.as_deref()) {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.strong("You own this world — World Editor");
                    ui.add_space(4.0);
                    egui::Grid::new("controls-editor-grid")
                        .num_columns(2)
                        .spacing([24.0, 4.0])
                        .show(ui, |ui| {
                            for row in EDITOR_ROWS {
                                ui.monospace(row.keys);
                                ui.label(row.action);
                                ui.end_row();
                            }
                        });
                    ui.add_space(4.0);
                    ui.small(
                        "The World/Local toggle beside the editor's transform fields \
                 switches the drag gizmo's orientation.",
                    );
                    // #1240 f170: aiming a gizmo freezes the avatar, and the key
                    // that releases it is the one the sheet already lists.
                    ui.small(
                        "While a gizmo is aimed your avatar is held still — Esc \
                 releases it.",
                    );
                }
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    if ui.button("Got it").clicked() {
                        panels.controls = false;
                    }
                });
            });
    });
    // Only track geometry once de-anchored — remembering the anchored
    // rect would persist "screen center" as the window's home.
    if panels.controls_seen
        && let Some(response) = response.as_ref()
    {
        chrome.remember(
            crate::ui::layout::UiWindow::Controls,
            response.response.rect,
        );
    }
    if !open {
        panels.controls = false;
    }
    // The latch that de-anchors the sheet lives in `latch_controls_seen`
    // (#1235 f36), NOT here: it used to sit at the bottom of this
    // function, behind the early return above, so only the two dismissals
    // that close from inside — the title-bar [x] and "Got it" — ever
    // reached it.
}

/// Latch the first-run treatment off on ANY dismissal of the Controls
/// sheet (#1235 f36).
///
/// While `controls_seen` is false the sheet is built with
/// `.anchor(CENTER_CENTER)`, which re-pins every frame and is therefore
/// immovable, and no rect is remembered. The only write of the flag used
/// to live at the BOTTOM of `controls_hint_ui`, behind its
/// `if !panels.controls { return; }` — so a user who dismissed with Esc
/// (the key the sheet itself advertises) or with the toolbar toggle set
/// `panels.controls = false` from elsewhere, the next run early-returned,
/// and every reopen thereafter came back centre-pinned and undraggable.
/// The flag is persisted per machine (#820), so it survived restarts.
///
/// A falling edge rather than a render-path write, because the point is
/// that dismissal happens in three places and only one of them is the
/// renderer. Guarded (#879): the write only happens on the edge.
pub fn latch_controls_seen(mut panels: ResMut<UiPanels>, mut was_open: Local<bool>) {
    let open = panels.controls;
    if *was_open && !open && !panels.controls_seen {
        panels.controls_seen = true;
    }
    *was_open = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_resolve_to_the_matching_chassis() {
        assert_eq!(
            piloted_chassis(true, false, false, false, false),
            PilotedChassis::Boat
        );
        assert_eq!(
            piloted_chassis(false, true, false, false, false),
            PilotedChassis::Skiff
        );
        assert_eq!(
            piloted_chassis(false, false, true, false, false),
            PilotedChassis::Airship
        );
        assert_eq!(
            piloted_chassis(false, false, false, true, false),
            PilotedChassis::Airplane
        );
    }

    /// #1266 f167. THE SEQUENCE: pick "Car" in Avatar › Locomotion, open
    /// Controls to learn the keys, and the sheet says you are piloting a
    /// "Skiff" — so you go looking for the skiff you did not choose. Only
    /// Airplane agreed, and this window's own footer sends the reader
    /// straight into the mismatch.
    ///
    /// This asserts the SHARED SOURCE rather than the five strings: a
    /// second list of names is the defect, so a test that spells them
    /// again would be the defect in test form.
    #[test]
    fn the_controls_sheet_names_a_chassis_the_picker_offers() {
        use crate::pds::avatar::locomotion::LocomotionConfig;

        let offered: Vec<&str> = LocomotionConfig::pickers()
            .iter()
            .map(|(_, label, _)| *label)
            .collect();

        for chassis in [
            PilotedChassis::OnFoot,
            PilotedChassis::Boat,
            PilotedChassis::Skiff,
            PilotedChassis::Airship,
            PilotedChassis::Airplane,
        ] {
            assert!(
                offered.contains(&chassis.label()),
                "the sheet says {:?} but the picker offers {offered:?}",
                chassis.label()
            );
        }

        // The control: the five are distinct, so a mapping that collapsed
        // two chassis onto one preset would not pass by accident.
        let mut named: Vec<&str> = [
            PilotedChassis::OnFoot,
            PilotedChassis::Boat,
            PilotedChassis::Skiff,
            PilotedChassis::Airship,
            PilotedChassis::Airplane,
        ]
        .iter()
        .map(|c| c.label())
        .collect();
        named.sort_unstable();
        named.dedup();
        assert_eq!(named.len(), 5, "two chassis share a name");

        // And the one arm with no preset behind it keeps its own words —
        // it must not borrow a name from a vehicle the record does not
        // describe (#1241 f161).
        assert!(!offered.contains(&PilotedChassis::Unrecognised.label()));
    }

    #[test]
    fn the_humanoid_marker_is_on_foot() {
        assert_eq!(
            piloted_chassis(false, false, false, false, true),
            PilotedChassis::OnFoot
        );
    }

    /// #1241 f161. Sequence: an avatar record written by a newer build
    /// names a locomotion preset this one does not model; land in the
    /// world and press W. `build_preset_components` inserts a bare
    /// collider and NO marker, every drive system is marker-queried, and
    /// the body is an inert falling cube — while the sheet reported
    /// "Piloting: On foot" and listed walk keys. Total immobility
    /// presented as normal movement is the worst combination of a dead
    /// end and a lie.
    #[test]
    fn a_body_with_no_preset_marker_is_not_on_foot() {
        let chassis = piloted_chassis(false, false, false, false, false);
        assert_eq!(chassis, PilotedChassis::Unrecognised);
        assert_eq!(chassis.label(), "Unknown vehicle");
        let rows = movement_rows(chassis);
        assert!(
            rows.iter()
                .any(|r| r.action.contains("Avatar › Locomotion")),
            "the only row must point at the surface that can fix it"
        );
        assert!(
            !rows.iter().any(|r| r.action.contains("walk")),
            "listing walk keys for a body that cannot move is the defect"
        );
    }

    #[test]
    fn every_chassis_has_a_non_empty_movement_sheet() {
        for chassis in [
            PilotedChassis::OnFoot,
            PilotedChassis::Boat,
            PilotedChassis::Skiff,
            PilotedChassis::Airship,
            PilotedChassis::Airplane,
            PilotedChassis::Unrecognised,
        ] {
            let rows = movement_rows(chassis);
            assert!(!rows.is_empty(), "{chassis:?} has no movement rows");
            for row in rows {
                assert!(!row.keys.is_empty(), "{chassis:?} row has empty keys");
                assert!(!row.action.is_empty(), "{chassis:?} row has empty action");
            }
        }
    }

    #[test]
    fn editor_rows_cover_the_core_gestures() {
        // #851's acceptance: a new owner learns the three core editing
        // gestures (plus the escape) from the sheet alone. Guard the rows
        // so a future trim can't silently drop one.
        let keys: Vec<&str> = EDITOR_ROWS.iter().map(|r| r.keys).collect();
        for expected in ["Right-click", "Left-click", "Shift-drag", "Esc"] {
            assert!(keys.contains(&expected), "editor rows lost {expected}");
        }
        for row in EDITOR_ROWS {
            assert!(!row.action.is_empty(), "{} row has empty action", row.keys);
        }
    }

    /// Every `KeyCode` `player::humanoid` reads, and the fragment
    /// [`ON_FOOT_ROWS`] must print for it. A key that appears in the
    /// handler and not here fails the test below by name: adding a
    /// binding without telling the sheet is exactly the drift #803's
    /// comment promised could not happen.
    const ON_FOOT_KEY_ROWS: &[(&str, &str)] = &[
        ("KeyW", "W A S D"),
        ("KeyA", "W A S D"),
        ("KeyS", "W A S D"),
        ("KeyD", "W A S D"),
        ("ArrowUp", "Arrows"),
        ("ArrowDown", "Arrows"),
        ("ArrowLeft", "Arrows"),
        ("ArrowRight", "Arrows"),
        ("Space", "Space"),
        ("ShiftLeft", "Shift"),
        ("ShiftRight", "Shift"),
        ("KeyC", "C"),
        // Ctrl is not bound on wasm (#839) and the row says so.
        (
            "ControlLeft",
            if cfg!(target_arch = "wasm32") {
                ""
            } else {
                "Ctrl"
            },
        ),
        (
            "ControlRight",
            if cfg!(target_arch = "wasm32") {
                ""
            } else {
                "Ctrl"
            },
        ),
    ];

    /// **The on-foot rows mirror the humanoid handler** (#1235 f40/f41).
    ///
    /// The #803 contract — "the sheet can never drift from the actual
    /// controls again, change both together" — was a comment, and the
    /// guards next to it pinned `GLOBAL_ROWS` and `EDITOR_ROWS` only.
    /// Nothing tested the MOVEMENT rows, and both halves drifted: #1193
    /// made Shift the run key on land while the sheet went on listing
    /// Shift as "swim down" and nothing else, and the Space row advertised
    /// a "climb" the humanoid controller has never implemented — a verb
    /// the user hunts for a surface to use, concluding the app is broken
    /// rather than the sheet wrong.
    ///
    /// Reads the handler's source rather than its behaviour, because
    /// "which keys does this system look at" is a property of the text and
    /// there is no harness that could answer it otherwise. It is the same
    /// idiom the glyph guard and the room-writer walk use.
    #[test]
    fn on_foot_rows_mirror_the_humanoid_handler() {
        let handler = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/player/humanoid.rs"),
        )
        .expect("the humanoid handler is readable");
        let printed: String = ON_FOOT_ROWS
            .iter()
            .map(|r| format!("{}\n{}\n", r.keys, r.action))
            .collect();

        let mut bound: Vec<&str> = Vec::new();
        let mut rest = handler.as_str();
        while let Some(at) = rest.find("KeyCode::") {
            rest = &rest[at + "KeyCode::".len()..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(rest.len());
            let key = &rest[..end];
            if !bound.contains(&key) {
                bound.push(key);
            }
        }
        assert!(
            bound.len() >= 8,
            "the handler scan found only {bound:?} — it has stopped working"
        );

        for key in bound {
            let (_, fragment) = ON_FOOT_KEY_ROWS
                .iter()
                .find(|(name, _)| *name == key)
                .unwrap_or_else(|| {
                    panic!(
                        "player::humanoid binds KeyCode::{key} and ON_FOOT_KEY_ROWS \
                         does not know it — say what it does on the sheet (or list \
                         it here with an empty fragment if it is deliberately \
                         undocumented)"
                    )
                });
            if fragment.is_empty() {
                continue;
            }
            assert!(
                printed.contains(fragment),
                "the sheet never prints {fragment:?} for KeyCode::{key}:\n{printed}"
            );
        }

        // Shift is the run key on land (#1193) and the sheet must say so —
        // it listed Shift ONLY as "swim down", so a careful reader came
        // away actively believing Shift does something else on land.
        assert!(
            handler.contains("let running ="),
            "the run branch moved; re-check what the sheet should say about Shift"
        );
        assert!(
            printed.contains("run"),
            "Shift-to-run is bound and the sheet does not mention running"
        );
        // …and it must not name a verb the handler does not have.
        assert!(
            !handler.to_lowercase().contains("climb"),
            "the humanoid handler grew a climb — the sheet may advertise one again"
        );
        assert!(
            !printed.to_lowercase().contains("climb"),
            "the sheet advertises a climb the humanoid controller does not implement"
        );
    }

    /// #1235 f36. Sequence: a brand-new user closes the auto-opened
    /// Controls sheet with Esc — the key the sheet itself lists under
    /// "back out" — and from then on every reopen lands dead centre and
    /// cannot be dragged, forever, persisted per machine. The latch used
    /// to live at the bottom of `controls_hint_ui`, behind its
    /// `if !panels.controls { return; }`, so only the [x] and "Got it"
    /// (which close from inside) reached it.
    #[test]
    fn any_dismissal_ends_the_first_run_pinning() {
        // `run_system_cached`, not `run_system_once`: the falling edge
        // lives in a `Local`, and a fresh system every call would have no
        // previous frame to fall from.
        let mut world = bevy::prelude::World::new();
        world.insert_resource(UiPanels::default());
        assert!(
            world.resource::<UiPanels>().controls,
            "precondition: the sheet opens itself on a first run"
        );

        // Open, drawn, still first-run.
        world
            .run_system_cached(latch_controls_seen)
            .expect("system runs");
        assert!(!world.resource::<UiPanels>().controls_seen);

        // Dismissed from OUTSIDE the renderer — the Esc ladder and the
        // toolbar toggle both look exactly like this.
        world.resource_mut::<UiPanels>().controls = false;
        world
            .run_system_cached(latch_controls_seen)
            .expect("system runs");
        assert!(
            world.resource::<UiPanels>().controls_seen,
            "Esc / the toolbar toggle must de-anchor the sheet too"
        );
    }

    /// #1237 f142. Sequence: leave your own world through a gateway with
    /// the World Editor open and a placement selected; arrive in a
    /// stranger's overland, and find a glowing green circle over their
    /// terrain that the gizmo can still drag and commit into their room's
    /// live record.
    ///
    /// `panels.world_editor` stays TRUE for a visitor — the toolbar
    /// renders a DISABLED button without clearing the flag, and
    /// `room_admin_ui` returns at its ownership gate before the reconcile
    /// that would — so a window-flag-only gate is not a gate at all here.
    /// Three surfaces answered the question three different ways and the
    /// overlay answered it not at all; this pins that they now share one.
    #[test]
    fn every_room_gizmo_surface_asks_the_same_ownership_question() {
        // Structural, not behavioural: an `AtprotoSession` fixture needs a
        // live DPoP-signing session, and what actually broke here was
        // WHICH question each surface asked, not the answer. Nothing may
        // go back to reading
        // `panels.world_editor` on its own, which is the shape of the bug.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for rel in [
            "src/editor_gizmo/sync.rs",
            "src/editor_gizmo/highlight.rs",
            "src/world_builder/mod.rs",
        ] {
            let src = std::fs::read_to_string(root.join(rel)).expect("source is readable");
            assert!(
                src.contains("can_edit_room()"),
                "{rel} must gate the room gizmo on RoomEditAccess"
            );
            assert!(
                !src.contains("panels.world_editor"),
                "{rel} reads the window flag directly again — that flag is true \
                 for a visitor standing in a stranger's world"
            );
        }
    }

    /// **The camera rows name gestures a laptop can perform** (#1242
    /// f166). Pan was middle-button-only with no modifier and no
    /// alternative, while the sheet advertised it unconditionally — a
    /// cheat-sheet row that cannot be performed is worse than no row.
    #[test]
    fn camera_rows_offer_a_reachable_pan_and_zoom() {
        let printed: String = CAMERA_ROWS
            .iter()
            .map(|r| format!("{}\n{}\n", r.keys, r.action))
            .collect();
        assert!(
            printed.contains("Alt"),
            "no trackpad-reachable pan: {printed}"
        );
        assert!(
            printed.contains("pinch"),
            "no trackpad-reachable zoom: {printed}"
        );
        assert!(printed.contains("orbit"));
    }

    /// **The visitor-usable right-click is documented to visitors**
    /// (#1235 f149). `AVATAR_ROWS` is rendered outside the ownership
    /// branch; `EDITOR_ROWS` inside it. The rows must not swap sides.
    #[test]
    fn the_avatar_right_click_row_is_not_owner_gated() {
        let src = include_str!("toolbar.rs");
        let owner_heading = src
            .find("You own this world — World Editor")
            .expect("the owner heading");
        let rendered = src
            .find("for row in AVATAR_ROWS")
            .expect("the avatar rows are rendered");
        assert!(
            rendered < owner_heading,
            "the avatar rows moved inside the owner-only block"
        );
        let printed: String = AVATAR_ROWS
            .iter()
            .map(|r| format!("{}\n{}\n", r.keys, r.action))
            .collect();
        for verb in ["take off", "re-seat", "wear"] {
            assert!(printed.to_lowercase().contains(verb), "{verb}: {printed}");
        }
    }

    /// #1261 f235: the account chip's label is unbounded, and it is the
    /// item the right-to-left layout protects hardest — so a long
    /// custom-domain handle spends width that Controls and Settings, at
    /// the tail of the same group, are the first to lose.
    #[test]
    fn a_long_handle_elides_to_the_part_that_names_a_person() {
        // Short enough to print whole: the overwhelmingly common case.
        assert_eq!(
            account_chip_label("alice.bsky.social"),
            "@alice.bsky.social"
        );
        // A legal ATProto handle is a domain, and a domain has no bound.
        let long = account_chip_label("someone.a-very-long-custom-domain.example");
        assert_eq!(long, "@someone…");
        assert!(long.chars().count() <= HANDLE_CHIP_MAX_CHARS);
        // Even when the first label alone is the whole problem.
        let head_only = account_chip_label(&"z".repeat(200));
        assert!(
            head_only.chars().count() <= HANDLE_CHIP_MAX_CHARS,
            "{head_only}"
        );
        assert!(head_only.starts_with('@') && head_only.ends_with('…'));
    }

    /// The width measurement must name buttons this file actually draws.
    ///
    /// [`trailing_needed`] decides whether the bar folds, and it decides
    /// it by laying out [`TRAILING_LABELS`]. If a label is renamed in the
    /// render and not here, the bar keeps reserving room for a button
    /// that no longer exists — or, worse, stops reserving room for one
    /// that does, which is the #1280 shape again: an arithmetic
    /// prediction about widgets, drifting away from the widgets.
    #[test]
    fn every_measured_label_is_a_label_this_file_draws() {
        let source = include_str!("toolbar.rs");
        for label in TRAILING_LABELS.iter().chain(LEADING_LABELS.iter()) {
            // The mute button builds its label from the live state glyph
            // ("🔇 Mute" / "🔊 Mute"), so for a glyph-prefixed label match
            // on the words after it. Everything else matches whole.
            let needle = match label.strip_prefix(|c: char| !c.is_ascii()) {
                Some(rest) => rest.trim_start(),
                None => label,
            };
            assert!(
                source.contains(&format!("\"{needle}\"")),
                "the width measurement names {label:?}, which this file no longer draws"
            );
        }
    }

    /// #1261 f235, measured with the app's own fonts rather than argued
    /// from the review's ~1100 pt estimate.
    ///
    /// THE SEQUENCE: a browser window narrowed to 1024 CSS px — or
    /// 1280x720 at 125% OS scaling, or two presses of the #1259 f239
    /// zoom. The bar is one non-wrapping `ui.horizontal`, so the tail of
    /// the right-to-left group runs leftward past its own rect and
    /// overpaints the toggles there. Losing that tail means losing
    /// Settings (the theme picker a low-vision user needs) and Controls,
    /// with no error and no keyboard alternative.
    ///
    /// The claim is a LOWER bound: it adds only the reserved-width
    /// constants and the labels both arrays name, and ignores separators
    /// and the account menu's own padding. The real bar is wider than
    /// this, so a failure here is unambiguous.
    #[test]
    fn the_bar_does_not_fit_a_1024_point_viewport_and_folds() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::ui::fonts::build_font_definitions(None));
        let mut measured = (0.0_f32, 0.0_f32);
        // Fonts are not available until the context has run a pass.
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1920.0, 200.0),
                )),
                ..Default::default()
            },
            |ui| {
                let leading = WORDMARK_WIDTH
                    + CHAT_TOGGLE_WIDTH
                    + PEOPLE_TOGGLE_WIDTH
                    + LEADING_LABELS
                        .iter()
                        .map(|label| button_width(ui, label))
                        .sum::<f32>();
                // A handle already elided by `account_chip_label`, so this
                // is the WIDEST the chip can be — not a pathological one.
                let chip = account_chip_label(&"z".repeat(200));
                measured = (leading, trailing_needed(ui, &chip));
            },
        );
        let (leading, trailing) = measured;
        assert!(leading > 0.0 && trailing > 0.0, "nothing was measured");
        assert!(
            leading + trailing > 1024.0,
            "the bar was supposed to overflow 1024 pt: {leading:.0} + {trailing:.0}"
        );
        assert!(
            leading + trailing < 1600.0,
            "and to fit a full-width desktop unfolded: {leading:.0} + {trailing:.0}"
        );
    }

    /// #1260 f248: no dead tab stop where nothing is drawn.
    ///
    /// The claim under test is egui's, not ours — `Sense::interactive()`
    /// is `CLICK | DRAG`, and only an interactive rect can take focus —
    /// so asserting the sense is asserting the tab stop.
    #[test]
    fn the_healthy_anomaly_slot_is_not_a_tab_stop() {
        assert!(
            !anomaly_slot_sense(false).interactive(),
            "a healthy session reserves the slot but must not be focusable in it"
        );
        assert!(
            anomaly_slot_sense(true).senses_click(),
            "with an anomaly showing, the dot is the click-through to Diagnostics"
        );
    }

    /// **The sheet names the unstuck command** (#1240 f159). It has no key
    /// binding — there is no free key that does not collide with movement
    /// — so the sheet is the only surface that can tell anyone it exists,
    /// and being unable to move is the failure a cheat-sheet most needs to
    /// answer.
    #[test]
    fn the_global_rows_name_the_way_out_of_being_stuck() {
        let printed: String = GLOBAL_ROWS
            .iter()
            .map(|r| format!("{}\n{}\n", r.keys, r.action))
            .collect();
        assert!(printed.contains("Return to spawn"), "{printed}");
    }

    /// **The sheet names every global shortcut the app binds** (#1141).
    ///
    /// The Controls sheet is the app's one onboarding surface — it opens
    /// itself on a first run. Undo/redo shipped with #864 and was never
    /// added here, so the only place it was written down was the hover
    /// text of a button inside an editor a first-session visitor has no
    /// reason to open. Pinning the key names here means the next binding
    /// added without a row fails the suite.
    #[test]
    fn global_rows_cover_every_bound_shortcut() {
        let keys: Vec<&str> = GLOBAL_ROWS.iter().map(|r| r.keys).collect();
        for expected in [
            "Enter",
            "Esc",
            "Ctrl+S",
            "Ctrl+Z / Ctrl+Shift+Z",
            "Ctrl+plus / Ctrl+minus",
        ] {
            assert!(keys.contains(&expected), "global rows lost {expected}");
        }
        for row in GLOBAL_ROWS {
            assert!(!row.action.is_empty(), "{} row has empty action", row.keys);
        }
    }

    /// **The emote hint names words that actually gesture** (#1141).
    ///
    /// The hint exists because the feature is otherwise invisible, so a
    /// hint that named a word the keyword table no longer carries would
    /// be worse than saying nothing. Round-trips each example word back
    /// through the matcher the chat send path uses.
    #[test]
    fn the_emote_hint_names_words_that_still_gesture() {
        use crate::player::emote::Emote;
        let hint = Emote::hint_line();
        for emote in Emote::ALL {
            let word = emote.keywords()[0];
            assert!(hint.contains(word), "hint dropped {word:?}: {hint}");
            assert_eq!(
                Emote::from_text(word),
                Some(emote),
                "{word:?} is advertised but no longer gestures"
            );
        }
    }

    #[test]
    fn ground_and_air_chassis_read_distinctly() {
        // The whole point of #803: the sheet is no longer a stale union. A
        // skiff shows a handbrake; an airship shows climb/descend + strafe;
        // they must not share a row set.
        assert_ne!(
            movement_rows(PilotedChassis::Skiff).len(),
            0,
            "skiff sheet is empty"
        );
        let skiff_actions: Vec<&str> = movement_rows(PilotedChassis::Skiff)
            .iter()
            .map(|r| r.action)
            .collect();
        assert!(
            skiff_actions.iter().any(|a| a.contains("handbrake")),
            "skiff sheet lost its handbrake row"
        );
        let airship_actions: Vec<&str> = movement_rows(PilotedChassis::Airship)
            .iter()
            .map(|r| r.action)
            .collect();
        assert!(
            airship_actions.iter().any(|a| a.contains("climb")),
            "airship sheet lost its climb/descend row"
        );
    }
}
