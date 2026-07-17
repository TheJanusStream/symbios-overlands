//! Symbios Overlands — library crate.
//!
//! This is the home for every gameplay module. The companion binary in
//! `src/main.rs` is a thin shim that just calls [`run`]; having a library
//! target lets integration tests in `tests/` import the public API directly.
//!
//! [`run`] wires every gameplay plugin, initialises the shared ECS resources,
//! and coordinates the three-stage state machine (`Login` → `Loading` →
//! `InGame`). The loading gate explicitly waits on **all six** loading
//! tasks — heightmap generation, the ATProto PDS room-record fetch, the
//! avatar-record fetch, the inventory-record fetch, the seeded
//! ambient-audio bake, *and* the room compile itself — before entering
//! `InGame`, so slower PDS round-trips cannot be silently dropped, gameplay
//! never runs with half-loaded recipes or a silent world, and the wasm
//! build's long synchronous compile stall stays behind the loading screen.

// This crate is an application, not a published library — the lib target
// exists so integration tests can import the module tree. Module docstrings
// freely reference sibling sub-modules (e.g. a "Sub-module map" listing
// internal helpers) for contributor navigation under
// `cargo doc --document-private-items`. Bumping those sub-modules to `pub`
// just to satisfy the lint would widen the real API surface, so we opt
// out of the warning at the crate root instead.
#![allow(rustdoc::private_intra_doc_links)]

use avian3d::PhysicsPlugins;
use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight, NotShadowCaster};
use bevy::log::LogPlugin;
#[cfg(not(target_arch = "wasm32"))]
use bevy::pbr::wireframe::WireframePlugin;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

pub mod alloc_track;
pub mod audio_materials;
pub mod audio_mute;
pub mod avatar;
pub mod boot_params;
pub mod camera;
pub mod catalogue;
pub mod clouds;
pub mod config;
pub mod diagnostics;
pub mod editor_gizmo;
pub mod interaction;
pub mod loading;
pub mod logout;
pub mod network;
pub mod oauth;
pub mod offload;
pub mod pds;
pub mod player;
pub mod prefs;
pub mod protocol;
pub mod seeded_defaults;
pub mod social;
pub mod splat;
pub mod state;
pub mod terrain;
pub mod ui;
pub mod urban;
pub mod water;
pub mod world_builder;

/// Headless render tool — drives the real spawn path to produce contact-sheet
/// PNGs for self-validating geometry/materials. Native-only (the harness pulls
/// the headless render stack); the web deploy never builds it.
#[cfg(not(target_arch = "wasm32"))]
pub mod render_tool;

/// Marker for the unlit sky cuboid spawned in `setup_lighting`. The world
/// compiler uses this to retint the sky material when a room record's
/// `environment.sky_color` changes.
#[derive(Component)]
pub struct SkyBox;

/// Pin the sky cuboid to the active camera each frame so its faces are
/// always equidistant from the viewer.
///
/// The cuboid is sized for a "looks-far-enough" backdrop (4 km × 2 km × 4 km
/// at the default `SKY_SCALE`), but it is fixed-size, not infinite. Without
/// this follow it stays anchored at world origin, so once the player moves
/// off-centre the closest face approaches the camera and its edges show up
/// as hard seams against the more-distant adjacent faces. Pinning the
/// cuboid's centre to the camera keeps every face at a constant distance
/// (half the cuboid's side on that axis) regardless of where the player
/// roams, which is what a backdrop should do anyway.
fn track_skybox_to_camera(
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<SkyBox>)>,
    mut skybox: Query<&mut Transform, With<SkyBox>>,
) {
    let Ok(cam_tx) = camera.single() else {
        return;
    };
    let cam = cam_tx.translation();
    for mut transform in skybox.iter_mut() {
        transform.translation = cam;
    }
}

pub use clouds::CloudLayer;

/// Format elapsed seconds as a `MM:SS` (or `H:MM:SS`) timestamp string.
pub fn format_elapsed_ts(elapsed_secs: f64) -> String {
    let total = elapsed_secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
use pds::{AvatarRecord, InventoryRecord, RoomRecord};
use state::{AppState, ChatHistory, LocalSettings, PublishFeedback};

/// Build the Bevy `App`, register every plugin, and block on
/// `App::run()`. Pulled out of `main.rs` so integration tests in `tests/`
/// can import the library's module tree without pulling in the binary's
/// execution entry point.
pub fn run() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    // Pulled before `App::new()` so the native `clap::Parser::parse()` can
    // emit `--help` / `--version` and exit cleanly without bringing up a
    // Bevy window first. WASM reads from the URL bar — no I/O risk.
    let boot = boot_params::detect();

    let fc = config::camera::fog::COLOR;
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgba(fc[0], fc[1], fc[2], fc[3])))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Symbios Overlands".into(),
                        prevent_default_event_handling: false,
                        ..default()
                    }),
                    // The [x] must not kill the process with unsaved edits
                    // aboard (#839): `intercept_window_close` routes it
                    // through the unsaved guard and exits via `AppExit`.
                    close_when_requested: false,
                    ..default()
                })
                // `webrtc_ice::agent::agent_internal` emits a `WARN` every
                // ~200ms during ICE bring-up whenever the agent has zero
                // candidate pairs ("pingAllCandidates called with no
                // candidate pairs"). This is expected behaviour — candidate
                // gathering + signalling of the remote side takes several
                // seconds, and the agent keeps retrying the pairing loop in
                // the meantime. Demote the whole agent_internal module to
                // `error` so the handshake log stays readable; genuine ICE
                // failures still surface via the `webrtc_ice` module's other
                // error-level events.
                .set(LogPlugin {
                    filter: format!(
                        "{},webrtc_ice::agent::agent_internal=error",
                        bevy::log::DEFAULT_FILTER
                    ),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default());
    // Native-only: WebGL2 lacks the POLYGON_MODE_LINE wgpu feature the
    // wireframe plugin depends on, and Overlands' WASM build can land on
    // either WebGPU or WebGL2 depending on the browser, so the safer
    // default is to omit the plugin entirely on web. The diagnostics
    // wireframe checkbox is gated under the same cfg.
    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(WireframePlugin::default());
    // wasm-only: preventDefault the browser's Ctrl+S/Cmd+S save dialog so
    // the in-app publish shortcut (#836) can own the chord. Installed as
    // a JS capture-phase listener because `prevent_default_event_handling`
    // stays false (F5 / Ctrl+R must keep working).
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Startup, ui::shortcuts::install_ctrl_s_blocker);
    // Exit guards (#839): closing the tab / native window used to bypass
    // the unsaved-edits guard entirely. Both run in every AppState —
    // without record resources the dirty set is empty and closing is
    // unprompted.
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Startup, ui::unsaved_guard::install_beforeunload_guard)
        .add_systems(Update, ui::unsaved_guard::sync_beforeunload_dirty);
    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(Update, ui::unsaved_guard::intercept_window_close);
    app.add_plugins(PhysicsPlugins::default())
        // BootParams must be resident before DiagnosticsPlugin::build reads it
        // for the boot StartupSnapshot, so insert it here and add the plugin
        // right after. The plugin opens the native session-log sink, records
        // the boot snapshot as seq 0, installs the crash-dump panic hook, and
        // registers the flush + legacy-forward systems.
        .insert_resource(boot)
        .add_plugins(diagnostics::DiagnosticsPlugin)
        // Metrics spine: registers Bevy's FrameTime/EntityCount/SystemInfo
        // diagnostics (previously unused) + scrapes them, asset/collider counts
        // and the ShapeMeshCache length into the shared MetricsRegistry at 1 Hz.
        .add_plugins(diagnostics::MetricsPlugin)
        .add_plugins(transform_gizmo_bevy::TransformGizmoPlugin)
        .add_plugins(MaterialPlugin::<clouds::CloudMaterial>::default())
        .add_plugins(terrain::TerrainPlugin)
        .add_plugins(world_builder::WorldBuilderPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(network::NetworkPlugin)
        .add_plugins(avatar::AvatarPlugin)
        .add_plugins(social::SocialPlugin)
        .add_plugins(logout::LogoutPlugin)
        .add_plugins(editor_gizmo::EditorGizmoPlugin)
        .add_plugins(interaction::InteractionPlugin)
        .add_plugins(audio_mute::AudioMutePlugin)
        .init_state::<AppState>()
        // Live anomaly engine — added after init_state so its OnEnter/OnExit
        // (Loading) systems bind to the now-registered state schedules. Reads
        // the metrics registry + ECS state each second and routes rule fires
        // into the session log + GUI badges.
        .add_plugins(diagnostics::anomaly::AnomalyPlugin)
        .init_resource::<ChatHistory>()
        .init_resource::<LocalSettings>()
        // One publish-status resource per editable record — never
        // shared, so publishing one no longer stamps another editor's
        // status line.
        .init_resource::<PublishFeedback<RoomRecord>>()
        .init_resource::<PublishFeedback<AvatarRecord>>()
        .init_resource::<PublishFeedback<InventoryRecord>>()
        .init_resource::<ui::toolbar::UiPanels>()
        .init_resource::<ui::layout::WindowLayout>()
        .init_resource::<ui::layout::LiveWindowRects>()
        .init_resource::<loading::fetch::RecordFetchOutcomes>()
        .init_resource::<ui::diagnostics::DiagTab>()
        .init_resource::<ui::shortcuts::PublishShortcut>()
        .init_resource::<ui::chat::ChatFocusRequest>()
        .init_resource::<ui::toast::Toasts>()
        .init_resource::<world_builder::grammar_diag::GrammarDiagnostics>()
        .init_resource::<ui::catalogue::CatalogueBrowser>()
        .init_resource::<ui::inventory::PendingGeneratorDrop>()
        .init_resource::<state::PendingOutgoingOffers>()
        .init_resource::<state::BusyAutoDeclines>()
        .init_resource::<state::MutedDids>()
        .init_resource::<ui::login::LoginError>()
        .init_resource::<ui::login::LoginUiLatch>()
        .init_resource::<ui::login::LoginPostFeed>()
        .add_systems(
            OnEnter(AppState::Login),
            (
                ui::login::reset_login_ui_latch,
                ui::login::start_login_feed_fetch,
                loading::clear_abort_flag,
            ),
        )
        .init_resource::<ui::room::RoomEditorState>()
        .init_resource::<ui::avatar::AvatarEditorState>()
        .init_resource::<loading::LiveAmbientConfig>()
        .init_resource::<loading::PlayingAmbient>()
        .init_resource::<loading::AmbientSettle>()
        .init_resource::<loading::AmbientRebakePending>()
        .init_resource::<editor_gizmo::GizmoFramePref>()
        .init_resource::<oauth::OauthClientRes>()
        .add_systems(
            EguiPrimaryContextPass,
            ui::login::login_ui.run_if(in_state(AppState::Login)),
        )
        .add_systems(
            Update,
            (
                ui::login::poll_begin_auth_task,
                ui::login::poll_complete_auth_task,
                ui::login::poll_login_feed_fetch,
                #[cfg(target_arch = "wasm32")]
                ui::login::check_wasm_callback,
                #[cfg(target_arch = "wasm32")]
                ui::login::check_wasm_resume,
                #[cfg(target_arch = "wasm32")]
                ui::login::poll_resume_task,
                #[cfg(not(target_arch = "wasm32"))]
                ui::login::poll_native_callback,
            )
                .run_if(in_state(AppState::Login)),
        )
        // Keep the relay service-auth token fresh for the whole logged-in
        // session so every WebRTC (re)connect — portal hop, dead-socket
        // respawn, network flap — presents a valid, unexpired token to the
        // relay instead of a stale one it rejects HTTP 401 (#714). Runs across
        // all post-login states (a portal hop passes through `Loading`), gated
        // on the session resource so it is inert before login / after logout.
        .add_systems(
            Update,
            (
                oauth::schedule_service_token_refresh,
                oauth::poll_service_token_refresh,
            )
                .run_if(resource_exists::<bevy_symbios_multiuser::auth::AtprotoSession>),
        )
        .add_systems(
            OnEnter(AppState::Loading),
            (
                diagnostics::plugin::record_session_snapshot,
                loading::reset_ambient_bake_state,
                loading::reset_fetch_outcomes,
                loading::start_room_record_fetch,
                loading::start_avatar_record_fetch,
                loading::start_inventory_record_fetch,
            ),
        )
        .add_systems(
            OnEnter(AppState::InGame),
            (
                loading::arm_ambient_settle,
                loading::toast_fetch_fallbacks,
                ui::toolbar::flash_owner_controls_once,
            ),
        )
        .add_systems(
            Update,
            (
                loading::abort_loading_to_login,
                loading::poll_record_task::<RoomRecord>,
                loading::poll_record_task::<AvatarRecord>,
                loading::poll_record_task::<InventoryRecord>,
                loading::fire_pending_record_retries::<RoomRecord>,
                loading::fire_pending_record_retries::<AvatarRecord>,
                // Inventory grew a real (2-attempt) retry budget in #840,
                // so its markers need a firing instance too — without
                // this one, a single transient inventory blip parked a
                // `PendingRecordRetry<InventoryRecord>` forever and the
                // gate hung on `LiveInventoryRecord` (#849).
                loading::fire_pending_record_retries::<InventoryRecord>,
                // Ambient bake is chained AFTER the room-record poll so
                // the dispatch sees `LiveRoomRecord` in the same frame
                // it arrives — without `.chain()` the starter would
                // miss the rising edge and stall the gate.
                loading::start_ambient_bake,
                loading::poll_ambient_task::<loading::AmbientBakeTask>,
                loading::check_loading_complete,
            )
                .chain()
                .run_if(in_state(AppState::Loading)),
        )
        // In-game ambient re-bake: editor edits (manual re-roll, Reset to
        // default, a direct audio edit) mutate `LiveRoomRecord`'s
        // `ambient_audio`, and this trio re-bakes + hot-swaps the looping
        // bed the same way the loading gate does on entry. Chained so a
        // finished bake's handle is swapped in the same frame it lands.
        .add_systems(
            Update,
            (
                loading::tick_ambient_settle,
                loading::rebake_ambient_on_record_change,
                loading::poll_ambient_task::<loading::AmbientRebakeTask>,
                loading::swap_ambient_player_to_handle,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            ui::loading::loading_ui.run_if(in_state(AppState::Loading)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (
                // The toolbar is chained first so its TopBottomPanel
                // claims screen space before any window lays out — egui
                // wants panels added before floating windows within a
                // frame. (The egui systems already serialise on the
                // shared context, so the chain costs no parallelism.)
                ui::toolbar::toolbar_ui,
                ui::diagnostics::diagnostics_ui,
                ui::chat::chat_ui,
                ui::people::people_ui,
                ui::people::incoming_offer_ui,
                ui::avatar::avatar_ui,
                ui::room::room_admin_ui,
                ui::inventory::inventory_ui,
                ui::catalogue::catalogue_ui,
                ui::toolbar::controls_hint_ui,
                // Travel visibility (#842): the in-flight overlay and the
                // portal approach prompt. Anchored HUD surfaces, so their
                // position in the window chain doesn't matter beyond
                // running after the toolbar carves the top strip.
                ui::travel::travel_overlay_ui,
                ui::travel::portal_prompt_ui,
                // Last in the chain AND on the egui Foreground order, so
                // toasts paint above every floating window (#819).
                ui::toast::toast_ui,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            ui::unsaved_guard::unsaved_guard_ui
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<ui::unsaved_guard::UnsavedGuard>),
        )
        // Global keyboard shortcuts (#836): Esc back-out ladder,
        // Enter-to-chat, Ctrl+S publish. Update (not the egui pass) so the
        // ladder's state checks land before the PostUpdate gizmo/blob Esc
        // handlers consume theirs.
        .add_systems(
            Update,
            ui::shortcuts::global_shortcuts.run_if(in_state(AppState::InGame)),
        )
        // Gateway destination picker (#748): the zone watcher opens/closes
        // the picker from the player's sensor overlap, the window renders
        // only while the picker resource is present.
        .add_systems(
            Update,
            ui::gateway::watch_gateway_zone.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            ui::gateway::gateway_picker_ui
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<ui::gateway::GatewayPicker>),
        )
        // Re-open chip while standing in a dismissed gateway zone (#842):
        // `GatewayDismissed` exists exactly between "picker closed" and
        // "walked out of the zone".
        .add_systems(
            EguiPrimaryContextPass,
            ui::gateway::gateway_reopen_chip_ui
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<ui::gateway::GatewayDismissed>),
        )
        .add_systems(
            Update,
            (
                ui::room::poll_publish_tasks,
                ui::avatar::poll_publish_avatar_tasks,
                ui::inventory::poll_publish_inventory_tasks,
                ui::inventory::handle_generator_drop,
                ui::inventory::preview_generator_drop,
                ui::inventory::open_people_for_gift_drag,
            )
                .run_if(in_state(AppState::InGame)),
        )
        // Machine-local UI prefs (#820): restore panels/settings from the
        // store once at startup, then persist them (debounced) whenever
        // they change. Runs in every AppState — panel toggles only happen
        // InGame, but the startup load must land before the first frame.
        .add_systems(Startup, prefs::load_prefs_at_startup)
        .add_systems(Update, prefs::save_prefs_when_changed)
        .add_systems(Startup, setup_lighting)
        .add_systems(
            Update,
            (clouds::track_cloud_layer_to_camera, track_skybox_to_camera),
        )
        .run();
}

fn setup_lighting(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cloud_materials: ResMut<Assets<clouds::CloudMaterial>>,
) {
    let lp = config::lighting::LIGHT_POS;
    // Start with the config default; `world_builder::compile_room_record`
    // patches the light with the `environment.sun_color` from the active
    // `RoomRecord` as soon as the recipe is compiled on InGame entry.
    let sc = config::lighting::SUN_COLOR;

    let cascade_shadow_config = CascadeShadowConfigBuilder {
        first_cascade_far_bound: config::lighting::CASCADE_FIRST_FAR,
        maximum_distance: config::lighting::CASCADE_MAX_DIST,
        ..default()
    }
    .build();

    commands.spawn((
        DirectionalLight {
            color: Color::srgb(sc[0], sc[1], sc[2]),
            shadows_enabled: true,
            illuminance: config::lighting::ILLUMINANCE,
            ..default()
        },
        Transform::from_xyz(lp[0], lp[1], lp[2]).looking_at(Vec3::ZERO, Vec3::Y),
        cascade_shadow_config,
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: config::lighting::AMBIENT_BRIGHTNESS,
        ..default()
    });

    // Sky — large unlit cuboid tinted by the distance fog.
    let sky_c = config::lighting::SKY_COLOR;
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(2.0, 1.0, 2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(sky_c[0], sky_c[1], sky_c[2]),
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(config::lighting::SKY_SCALE)),
        NotShadowCaster,
        SkyBox,
    ));

    // Cloud-deck — single horizontal `Plane3d` rendered through a custom
    // `MaterialExtension` over `StandardMaterial`. The mesh is large enough
    // (PLANE_HALF_EXTENT, default 4 km) that the plane edge sits well past
    // any reasonable `fog_visibility`, so the shader's horizon fade is the
    // only thing the camera ever sees at the plane boundary. Uniforms are
    // initialised from `Environment::default()` and re-patched by
    // `world_builder::compile::apply_environment_state` whenever the active
    // `RoomRecord` changes — same retint pattern as the `SkyBox` cuboid.
    let cc = config::lighting::clouds::COLOR;
    let csh = config::lighting::clouds::SHADOW_COLOR;
    let fc = config::camera::fog::COLOR;
    // Initial sun direction matches the directional light spawned above:
    // the light looks from `LIGHT_POS` toward the origin, so the unit
    // vector *toward* the sun is `normalize(LIGHT_POS)`. The world
    // compiler will refresh this each change tick from the live transform.
    let sun_v = Vec3::from_array(config::lighting::LIGHT_POS).normalize_or(Vec3::Y);
    let cloud_mat = cloud_materials.add(clouds::CloudMaterial {
        base: StandardMaterial {
            // The fragment shader replaces all colour math, so the base
            // colour is only used by fallback paths (e.g. shadow caster,
            // never wired here because shadows + prepass are disabled).
            base_color: Color::srgba(cc[0], cc[1], cc[2], 1.0),
            unlit: true,
            // Cull neither side — the underside is what the player sees
            // from below the deck, the topside is what they'd see if they
            // climbed above it on a tall airship.
            cull_mode: None,
            alpha_mode: AlphaMode::Blend,
            ..default()
        },
        extension: clouds::CloudExtension {
            uniforms: clouds::CloudUniforms {
                color: Vec4::new(cc[0], cc[1], cc[2], 1.0),
                shadow_color: Vec4::new(csh[0], csh[1], csh[2], 1.0),
                fog_color: Vec4::new(fc[0], fc[1], fc[2], fc[3]),
                sun_dir: Vec4::new(sun_v.x, sun_v.y, sun_v.z, 0.0),
                wind_dir: Vec2::from_array(config::lighting::clouds::WIND_DIR),
                cover: config::lighting::clouds::COVER,
                density: config::lighting::clouds::DENSITY,
                softness: config::lighting::clouds::SOFTNESS,
                speed: config::lighting::clouds::SPEED,
                scale: config::lighting::clouds::SCALE,
            },
        },
    });
    let half = config::lighting::clouds::PLANE_HALF_EXTENT;
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(half * 2.0, half * 2.0))),
        MeshMaterial3d(cloud_mat),
        Transform::from_xyz(0.0, config::lighting::clouds::HEIGHT, 0.0),
        NotShadowCaster,
        CloudLayer,
    ));
}
