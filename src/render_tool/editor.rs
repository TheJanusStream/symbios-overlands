//! `--editor`: the game's own editing surfaces over a compiled `--world`.
//!
//! The other modes show what a world is; this one shows what its owner does
//! to it. The toolbar, the World Editor, the Catalogue, the scene context
//! menu, the toast stack and the in-world transform gizmo are the game's own
//! systems, registered here under the game's own `in_state(AppState::InGame)`
//! run condition and reading the resources the game reads. So are the
//! gestures' consequences: the undo capture and apply systems, the drop
//! handler with its ground ring, the Catalogue's item-preview stage, and
//! avian's collider tree, which the drop's terrain ray and the scene pick
//! read. The interface draws through a `PrimaryEguiContext` on the rig camera
//! and the gizmo through its `GizmoCamera` marker, so both land in the same
//! off-screen target the drive loop reads back: one frame, world, interface
//! and handles together.
//!
//! One thing stands in. The World Editor is owner-only (`session.did ==
//! CurrentRoomDid`), and no OAuth handshake can run in a headless tool, so
//! [`stand_in_session`] inserts an offline `AtprotoSession` for the world's
//! own DID, built the way the crate's tests build theirs, with every URL on
//! `example.invalid` so nothing it names can resolve. Nothing registered
//! here performs network I/O: the lazy CJK font fetch is left out, the peer
//! messages a drop writes have no transport to go to, and the interface draws
//! with the bundled base faces and the game's theme.
//!
//! Two clocks and one window are the tool's to supply. bevy_egui stamps
//! every pass with `Time<Real>`, which would put wall-clock seconds into
//! window fades and tooltip delays while the scene steps by `1 / fps`, so
//! [`feed_egui`] overwrites the stamp with the tool's hand-driven clock. And
//! the gizmo crate and the editor's scene pick read the cursor and scale
//! factor from the primary window, which a headless app does not have, so
//! [`spawn_pointer_window`] spawns one: no winit, no surface, nothing renders
//! to it, sized to the frame the camera renders.
//!
//! `--editor-window` seeds a window's rect the way a prefs file restores one,
//! and `--editor-script` plays gestures on the clock: see [`script`].

pub(super) mod script;

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowResolution};
use bevy_egui::{
    EguiContext, EguiInput, EguiPlugin, EguiPostUpdateSet, EguiPreUpdateSet,
    EguiPrimaryContextPass, PrimaryEguiContext,
};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::{Broadcast, SendTo};

use crate::oauth::OauthRefreshCtx;
use crate::pds::{Placement, RoomRecord};
use crate::protocol::OverlandsMessage;
use crate::state::{AppState, LiveRoomRecord, PublishFeedback, StoredRoomRecord};
use crate::ui;
use crate::ui::layout::{UiWindow, WindowLayout};
use crate::ui::room::{EditorTab, GenNodeId, RoomEditorState};

use super::headless::{Clock, RenderJob};
use script::EditorScript;

/// Present when the world camera carries the egui context and the gizmo
/// camera marker.
#[derive(Resource)]
pub(super) struct EditorHost;

/// `--editor-tab` / `--editor-select` / `--editor-ui-scale` /
/// `--editor-window`: where the editor stands when the shot opens.
#[derive(Resource, Clone, Debug, Default)]
pub(super) struct EditorOpening {
    pub(super) tab: EditorTab,
    /// An item name in the record: its tree row on the Items tab, its first
    /// absolute placement on the Placements tab.
    pub(super) select: Option<String>,
    /// The Settings window's Interface scale, applied through
    /// `LocalSettings::ui_scale` exactly as the slider applies it.
    pub(super) ui_scale: Option<f32>,
    /// Window rects seeded into the persisted layout, by layout key.
    pub(super) windows: Vec<(&'static str, [f32; 4])>,
}

/// Every managed window, so `--editor-window` can name any of them.
const WINDOWS: [UiWindow; 10] = [
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

/// Parse `--editor-tab` by the tab's visible label.
pub(super) fn parse_tab(s: &str) -> Result<EditorTab, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "environment" => Ok(EditorTab::Environment),
        "items" => Ok(EditorTab::Generators),
        "placements" => Ok(EditorTab::Placements),
        "effects" => Ok(EditorTab::Effects),
        "raw" => Ok(EditorTab::Raw),
        _ => Err(format!(
            "--editor-tab {s:?}: expected environment, items, placements, effects or raw"
        )),
    }
}

/// Parse `--editor-window`: `<window>=x,y,w,h`, the window named by its
/// layout key, the rect in frame pixels at an interface scale of one.
pub(super) fn parse_window(s: &str) -> Result<(&'static str, [f32; 4]), String> {
    let bad = || {
        let keys: Vec<&str> = WINDOWS.iter().map(|w| w.key()).collect();
        format!(
            "--editor-window {s:?}: expected <window>=x,y,w,h with the window one of {}",
            keys.join(", ")
        )
    };
    let (name, rect) = s.split_once('=').ok_or_else(bad)?;
    let key = WINDOWS
        .iter()
        .map(|w| w.key())
        .find(|k| *k == name.trim())
        .ok_or_else(bad)?;
    let v: Vec<f32> = rect
        .split(',')
        .map(|p| p.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .map_err(|_| bad())?;
    match v.as_slice() {
        [x, y, w, h] if *w > 0.0 && *h > 0.0 => Ok((key, [*x, *y, *w, *h])),
        _ => Err(bad()),
    }
}

/// Register the editing surfaces over a world `world::register` has
/// already registered: `record` is the world's own record, which also
/// stands as the stored copy (a fresh sign-in with nothing edited yet), and
/// `did` its owner. `script`, when given, plays on the tool's clock.
pub(super) fn register(
    app: &mut App,
    record: &RoomRecord,
    did: &str,
    opening: EditorOpening,
    script: Option<EditorScript>,
) {
    if let Some(scale) = opening.ui_scale {
        use crate::config::ui::{UI_SCALE_MAX, UI_SCALE_MIN};
        assert!(
            (UI_SCALE_MIN..=UI_SCALE_MAX).contains(&scale),
            "--editor-ui-scale {scale}: the Settings slider runs {UI_SCALE_MIN} to {UI_SCALE_MAX}"
        );
        // `world::register` inserted the settings; `theme::sync_ui_scale`
        // pushes this into the egui context on its first frame.
        app.world_mut()
            .resource_mut::<crate::state::LocalSettings>()
            .ui_scale = scale;
    }
    let mut layout = WindowLayout::default();
    for (key, rect) in &opening.windows {
        layout.rects.insert((*key).to_string(), *rect);
    }
    app.add_plugins(EguiPlugin::default())
        .insert_resource(crate::camera::egui_global_settings())
        .insert_state(AppState::InGame)
        .add_plugins(transform_gizmo_bevy::TransformGizmoPlugin)
        .add_plugins(crate::editor_gizmo::EditorGizmoPlugin)
        // The Catalogue's picture of the selected entry.
        .add_plugins(crate::item_preview::ItemPreviewPlugin)
        // The collider tree a drop and the scene pick find the ground with.
        .add_plugins(avian3d::prelude::PhysicsPlugins::default())
        .insert_resource(EditorHost)
        .insert_resource(opening)
        .insert_resource(layout)
        .insert_resource(stand_in_session(did))
        .insert_resource(stand_in_refresh_ctx())
        .insert_resource(StoredRoomRecord(record.clone()))
        // A returning owner: the Controls sheet was dismissed long ago and
        // the World Editor is the panel being shown.
        .insert_resource(ui::toolbar::UiPanels {
            world_editor: true,
            controls: false,
            controls_seen: true,
            owner_hint_seen: true,
            ..default()
        })
        // What the surfaces below read, as `lib.rs::run` registers it.
        .init_resource::<ui::theme::CurrentTheme>()
        .init_resource::<crate::notify::Toasts>()
        .init_resource::<crate::state::ChatHistory>()
        .init_resource::<crate::audio_mute::AudioMuted>()
        .init_resource::<crate::diagnostics::anomaly::InvariantRegistry>()
        .init_resource::<ui::diagnostics::DiagTab>()
        .init_resource::<ui::layout::PanelFreeRect>()
        .init_resource::<ui::layout::LiveWindowRects>()
        .init_resource::<crate::network::LinkState>()
        .init_resource::<crate::avatar::BskyProfileCache>()
        .init_resource::<crate::boot_params::ClipboardQueue>()
        .init_resource::<crate::player::PlayerMoveRequest>()
        .init_resource::<ui::travel::WorldNames>()
        .init_resource::<bevy::pbr::wireframe::WireframeConfig>()
        .init_resource::<RoomEditorState>()
        .init_resource::<ui::avatar::AvatarEditorState>()
        .init_resource::<crate::editor_gizmo::GizmoFramePref>()
        .init_resource::<PublishFeedback<RoomRecord>>()
        .init_resource::<crate::world_builder::grammar_diag::GrammarDiagnostics>()
        .init_resource::<ui::shortcuts::PublishShortcut>()
        .init_resource::<ui::undo::RoomUndoHistory>()
        .init_resource::<ui::undo::AvatarUndoHistory>()
        .init_resource::<ui::undo::UndoShortcut>()
        .init_resource::<ui::undo::PendingUndoLabels>()
        .init_resource::<crate::state::RoomWriteSignals>()
        .init_resource::<crate::interaction::audio::AudioClipCache>()
        .init_resource::<bevy_symbios_audio::ui::AudioMonitor>()
        .init_resource::<ui::catalogue::CatalogueBrowser>()
        .init_resource::<ui::inventory::PendingGeneratorDrop>()
        .init_resource::<crate::state::PendingOutgoingOffers>()
        .init_resource::<crate::network::chunk::OutboundChunkSeq>()
        .init_resource::<script::Pointer>()
        .init_resource::<script::ScriptEguiEvents>()
        .add_message::<bevy_symbios_audio::ui::MonitorRequest>()
        .add_message::<bevy_symbios_audio::ui::MonitorControl>()
        .add_message::<Broadcast<OverlandsMessage>>()
        .add_message::<SendTo<OverlandsMessage>>()
        .add_systems(Startup, spawn_pointer_window)
        .add_systems(
            Update,
            (
                open_the_editor,
                ui::theme::sync_theme_from_settings,
                ui::theme::apply_theme_on_change,
                ui::theme::sync_ui_scale,
                ui::fonts::install_base_fonts,
            )
                .chain(),
        )
        .add_systems(
            PreUpdate,
            (
                feed_egui
                    .after(EguiPreUpdateSet::ProcessInput)
                    .before(EguiPreUpdateSet::BeginPass),
                ui::catalogue::mirror_preview_request,
            ),
        )
        .add_systems(
            Update,
            (
                ui::undo::apply_undo_shortcut,
                ui::inventory::handle_generator_drop,
                ui::inventory::preview_generator_drop,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            PostUpdate,
            ui::undo::capture_room_history
                .after(EguiPostUpdateSet::EndPass)
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (
                ui::toolbar::toolbar_ui,
                ui::room::room_admin_ui,
                ui::catalogue::catalogue_ui,
                ui::toast::toast_ui,
                script::paint_cursor,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    if let Some(script) = script {
        app.insert_resource(script::ScriptProgress {
            setup_done: script.start == 0,
        })
        .insert_resource(script)
        .init_resource::<script::ScriptRunner>()
        .add_systems(First, script::run_script.after(super::headless::tick_clock));
    }
}

/// The primary window the pointer lives in: the frame's size at a scale
/// factor of one, so a cursor position in it is a pixel of the frame.
fn spawn_pointer_window(mut commands: Commands, job: Res<RenderJob>) {
    let (width, height) = job.tile;
    commands.spawn((
        Window {
            resolution: WindowResolution::new(width, height).with_scale_factor_override(1.0),
            focused: true,
            ..default()
        },
        PrimaryWindow,
    ));
}

/// Stamp this pass with the tool's clock rather than the wall clock, say the
/// viewport has focus as a player's focused window would, hand over the
/// events a script frame produced, and keep the AccessKit tree a script
/// finds its widgets in.
pub(super) fn feed_egui(
    clock: Res<Clock>,
    mut events: ResMut<script::ScriptEguiEvents>,
    mut contexts: Query<(&mut EguiInput, &mut EguiContext), With<PrimaryEguiContext>>,
) {
    for (mut input, mut context) in &mut contexts {
        input.0.time = Some(f64::from(clock.elapsed));
        input.0.focused = true;
        if !events.0.is_empty() {
            input.0.events.append(&mut events.0);
        }
        context.get_mut().enable_accesskit();
    }
}

/// Put the World Editor where [`EditorOpening`] asks, once, through the
/// fields a click on the tab and on the item's row would set.
fn open_the_editor(
    opening: Res<EditorOpening>,
    record: Res<LiveRoomRecord>,
    mut editor: ResMut<RoomEditorState>,
    mut done: Local<bool>,
) {
    if std::mem::replace(&mut *done, true) {
        return;
    }
    editor.selected_tab = opening.tab;
    let Some(name) = opening.select.as_deref() else {
        return;
    };
    assert!(
        record.0.generators.contains_key(name),
        "--editor-select {name:?}: the record has no item by that name (--describe lists them)"
    );
    if opening.tab == EditorTab::Placements {
        let index = absolute_placement_of(&record.0, name).unwrap_or_else(|| {
            panic!("--editor-select {name:?}: no absolute placement puts it in the world")
        });
        editor.selected_placement = Some(index);
    } else {
        editor.selected_tab = EditorTab::Generators;
        editor.tree.selection.root = Some(name.to_string());
        editor.tree.selection.path = Some(Vec::new());
        editor.tree.view.set_one_selected(GenNodeId::root(name));
        editor.tree.pending_focus = true;
    }
}

/// The first placement that stands `name` in the world on its own - a
/// building, rather than one tree of a scatter.
fn absolute_placement_of(record: &RoomRecord, name: &str) -> Option<usize> {
    record.placements.iter().position(
        |p| matches!(p, Placement::Absolute { generator_ref, .. } if generator_ref == name),
    )
}

/// The world's owner, signed in without a network: the same shape the
/// crate's own tests build (`ui::room` placement-focus tests,
/// `ui::logout` teardown tests). Every URL is on `example.invalid`, a name
/// reserved never to resolve.
///
/// Built on the capped transport every production session gets (#1176)
/// rather than the tests' `::new`, which installs proto-blue's uncapped
/// fetcher. Nothing here fetches, but the rule is held over the source
/// (`no_production_path_builds_an_uncapped_oauth_transport`), and this tool
/// is source like any other.
fn stand_in_session(did: &str) -> AtprotoSession {
    use crate::oauth::capped_fetch::CappedFetcher;
    use proto_blue_oauth::types::TokenSet;
    use proto_blue_oauth::{DpopKey, DpopNonceCache, OAuthSession};

    let token_set = TokenSet {
        issuer: "https://example.invalid".into(),
        sub: did.into(),
        scope: "atproto".into(),
        access_token: "render-tool".into(),
        refresh_token: None,
        token_type: "DPoP".into(),
        expires_at: None,
        aud: None,
    };
    AtprotoSession {
        did: did.into(),
        handle: "you.example".into(),
        pds_url: "https://example.invalid".into(),
        session: std::sync::Arc::new(OAuthSession::with_fetch_handler(
            token_set,
            DpopKey::generate().expect("an ES256 key generates offline"),
            DpopNonceCache::new(),
            std::sync::Arc::new(CappedFetcher::new()),
        )),
    }
}

/// The refresh context the World Editor requires beside the session. Never
/// used: nothing here refreshes a token or publishes.
fn stand_in_refresh_ctx() -> OauthRefreshCtx {
    let server_metadata = serde_json::from_str(
        r#"{"issuer":"https://example.invalid",
            "authorization_endpoint":"https://example.invalid/authorize",
            "token_endpoint":"https://example.invalid/token"}"#,
    )
    .expect("the stand-in server metadata parses");
    OauthRefreshCtx {
        client: crate::oauth::OauthClientRes::default().0,
        server_metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy_egui::egui;

    #[test]
    fn a_tab_is_named_by_its_visible_label() {
        assert_eq!(parse_tab("Items"), Ok(EditorTab::Generators));
        assert_eq!(parse_tab(" placements "), Ok(EditorTab::Placements));
        assert_eq!(parse_tab("raw"), Ok(EditorTab::Raw));
        let err = parse_tab("region assets").unwrap_err();
        assert!(err.contains("--editor-tab"), "{err}");
    }

    #[test]
    fn a_selected_item_opens_on_its_first_absolute_placement() {
        let record = RoomRecord::default_for_seed(253, "did:render:253");
        let at = absolute_placement_of(&record, "landmark").expect("seed 253 places a landmark");
        assert!(matches!(
            &record.placements[at],
            Placement::Absolute { generator_ref, .. } if generator_ref == "landmark"
        ));
        // A scattered item has no absolute placement for the gizmo to take.
        let scattered = record
            .placements
            .iter()
            .find_map(|p| match p {
                Placement::Scatter { generator_ref, .. }
                    if absolute_placement_of(&record, generator_ref).is_none() =>
                {
                    Some(generator_ref.clone())
                }
                _ => None,
            })
            .expect("seed 253 scatters an item it never places on its own");
        assert_eq!(absolute_placement_of(&record, &scattered), None);
    }

    /// Exhaustive on purpose: a new managed window fails to compile here
    /// until `WINDOWS` names it, so `--editor-window` can always reach it.
    #[test]
    fn every_managed_window_can_be_named() {
        fn listed(window: UiWindow) -> bool {
            match window {
                UiWindow::Chat
                | UiWindow::People
                | UiWindow::Avatar
                | UiWindow::Inventory
                | UiWindow::Catalogue
                | UiWindow::WorldEditor
                | UiWindow::Diagnostics
                | UiWindow::AudioEditor
                | UiWindow::Controls
                | UiWindow::Settings => WINDOWS.contains(&window),
            }
        }
        for window in WINDOWS {
            assert!(listed(window));
            assert_eq!(
                parse_window(&format!("{}=1,2,3,4", window.key())),
                Ok((window.key(), [1.0, 2.0, 3.0, 4.0]))
            );
        }
        for bad in [
            "world_editor",
            "worldeditor=1,2,3,4",
            "catalogue=1,2,3",
            "chat=0,0,0,5",
        ] {
            let err = parse_window(bad).unwrap_err();
            assert!(err.contains("--editor-window"), "{bad}: {err}");
        }
    }

    /// A seeded rect is where the window opens: the persisted-layout path a
    /// prefs file restores through, read by the same `WindowChrome::place`
    /// every window calls.
    #[test]
    fn a_seeded_window_rect_is_where_the_window_opens() {
        let mut world = World::new();
        let (key, rect) = parse_window("world_editor=10,40,560,600").unwrap();
        let mut layout = WindowLayout::default();
        layout.rects.insert(key.to_string(), rect);
        world.insert_resource(layout);
        world.init_resource::<ui::layout::LiveWindowRects>();
        world.init_resource::<ui::layout::PanelFreeRect>();
        world.init_resource::<bevy::diagnostic::FrameCount>();
        let placed = world
            .run_system_once(|chrome: ui::layout::WindowChrome| {
                let ctx = egui::Context::default();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 720.0),
                    )),
                    ..Default::default()
                };
                let mut placed = None;
                let _ = ctx.run_ui(input, |ui| {
                    placed = Some(chrome.place(UiWindow::WorldEditor, ui.ctx()));
                });
                placed.expect("the pass ran")
            })
            .expect("the window chrome builds from its resources");
        assert_eq!(placed, (egui::pos2(10.0, 40.0), egui::vec2(560.0, 600.0)));
    }

    #[test]
    fn the_stand_in_owns_the_world_it_signs_into_and_no_other() {
        let session = stand_in_session("did:render:253");
        let room = |did: &str| crate::state::CurrentRoomDid(did.to_string());
        assert!(ui::toolbar::owns_current_room(
            Some(&session),
            Some(&room("did:render:253"))
        ));
        assert!(!ui::toolbar::owns_current_room(
            Some(&session),
            Some(&room("did:render:7"))
        ));
        assert!(session.pds_url.ends_with(".invalid"), "{}", session.pds_url);
    }

    /// The pass reads the tool's clock and a script frame's events, and the
    /// events are handed over once.
    #[test]
    fn egui_is_fed_the_tool_clock_and_the_script_events_once() {
        let mut world = World::new();
        world.insert_resource(Clock {
            step: 0.08,
            run: false,
            once: false,
            stepped: false,
            elapsed: 4.0,
        });
        world.insert_resource(script::ScriptEguiEvents(vec![egui::Event::PointerMoved(
            egui::pos2(3.0, 4.0),
        )]));
        let context = world
            .spawn((
                EguiInput::default(),
                EguiContext::default(),
                PrimaryEguiContext,
            ))
            .id();
        world.run_system_once(feed_egui).expect("feed runs");
        world.run_system_once(feed_egui).expect("feed runs again");
        let input = world.get::<EguiInput>(context).unwrap();
        assert_eq!(input.0.time, Some(4.0));
        assert!(input.0.focused);
        assert_eq!(input.0.events.len(), 1, "{:?}", input.0.events);
        assert!(world.resource::<script::ScriptEguiEvents>().0.is_empty());
    }
}
