//! Item preview - a live 3D picture of the selected catalogue entry
//! (#1288) or stash item (#1301).
//!
//! The Catalogue lists 392 entries by name and description alone, so the
//! only way to find out what one looks like was to copy it into the stash
//! and wear or place it; the Inventory was the same list of names. This
//! module renders the *selected* item into an off-screen texture that both
//! windows' detail panes draw as an ordinary `egui::Image`, which is why it
//! is not [`crate::ui::avatar::draw_avatar_icon`]'s neighbour: the
//! profile-picture path uploads bytes fetched from a PDS, and this one owns
//! a camera.
//!
//! The owner chose render-on-selection over a baked atlas: nothing here
//! goes stale when [`crate::catalogue::ENTRIES`] gains a row or #972
//! reworks one, and no image asset enters the repo or the wasm bundle.
//! The accepted cost is that the picture exists only for the SELECTED
//! item - the browse tree and the Inventory's rows stay text-only - and
//! that there is ONE stage: with both windows open and both holding a
//! selection, the most recent pick is the one pictured ([`wanted_subject`]).
//!
//! A vehicle avatar in the Catalogue (#1840) is a third subject, pictured
//! from the visuals tree its recipe builds. A person, a beast or a winged
//! body - an avatar item's or an avatar entry's - is pictured as the body
//! itself (#1842; see below), so choosing one no longer means wearing it to
//! see it.
//!
//! ## A stash item is editable, a catalogue entry is not
//!
//! A catalogue entry is a `const`, so its picture can never go stale. A
//! stash item can change under the same name, so its subject carries the
//! stash's change tick ([`PreviewSubject::Inventory`]) and a write to the
//! stash restages it once. That makes the tick load-bearing: a draw that
//! took `&mut` of the stash every frame without editing it would restage
//! the picture every frame, which is exactly what #1322 removed from the
//! two editors that did.
//!
//! ## The layer is the isolation; the distance is for something else
//!
//! The preview subject is spawned through the same avatar visual path a
//! worn prop uses, into a "stage" rooted at [`STAGE_ORIGIN`], and
//! everything on it carries [`PREVIEW_LAYER`]. The layer is what makes the
//! world camera blind to the stage and the preview camera blind to the
//! world: it is the isolation, and the distance is not.
//!
//! ## Two cameras, one egui context
//!
//! bevy_egui, by default, attaches its primary context to whichever
//! camera it meets first, and with this camera and the world camera both
//! spawning in `Startup` that was a race the wasm build lost (#1317):
//! egui drew through this inactive camera and the login screen went
//! blank. [`crate::camera`] now names the world camera as egui's and turns
//! the automatic pick off, so this camera must never carry
//! `PrimaryEguiContext`.
//!
//! **`RenderLayers` does not propagate down the hierarchy in Bevy 0.19** -
//! only `Visibility` does - and the spawn path builds a whole tree, whose
//! particle emitters go on adding children later. So the stage root
//! carries `Propagate<RenderLayers>` and the app registers
//! [`bevy::app::HierarchyPropagatePlugin`] for it in
//! `PostUpdate`, ordered before `VisibilitySystems::CheckVisibility`, so
//! the component lands on descendants - including ones spawned this frame -
//! before anything decides what the world camera can see. Inserting the layer
//! by hand at spawn time would have meant threading it through
//! `spawn_visual_tree` and every emitter path, and would still have missed
//! the particles.
//!
//! The distance earns its keep separately: a construct's audio is
//! `spatial: true` (see [`crate::world_builder::spatial_audio`]), so a
//! stage two kilometres above the listener is **silent** without this
//! module knowing anything about audio. Parking it at the origin would
//! have played every sounding item's loop the moment it was selected.
//! Two kilometres is also close enough that `f32` world coordinates stay
//! sub-millimetre, which the far side of the main camera's far plane would
//! not be.
//!
//! ## What it costs, which is a wasm question first
//!
//! One 256 px `Rgba8UnormSrgb` target - a quarter of a megabyte of VRAM,
//! declared `RenderAssetUsages::RENDER_WORLD` so it keeps no main-world
//! CPU copy (#565 measured that retention as the dominant wasm cost, and
//! a wasm heap never shrinks). The camera is spawned ONCE and toggled by
//! [`restage_preview`], never spawned per selection, and it is inactive
//! whenever no open window holds a selection it can picture - so a session
//! that never selects anything pays the target's memory and no passes at
//! all. `Msaa` is
//! off on every build, for the reason the WebGL2 build's main camera draws
//! FXAA in its place ([`crate::camera::EdgeSmoothing`]): Bevy's default
//! `Sample4` panics on the WebGL2 entry point.
//!
//! It adds no sampler to any existing material, so the WebGL2 16-sampler
//! ceiling the splat material sits against is untouched - this is a
//! second view, not a second texture on the first one.
//!
//! ## A person, a beast or a winged body is built, not spawned
//!
//! A rigged body has no tree to hand the spawn path; it is meshed by the
//! engine, which takes a few hundred milliseconds. The stage asks for it
//! exactly as the game asks for a player's body - and nothing here builds or
//! poses one a second way:
//!
//! - **Built by the game's own job**
//!   ([`crate::player::visuals::rigged_build_job`], far hair tier included)
//!   through [`crate::offload::offload`], which on the web is the worker: the
//!   main thread never meshes a body. At the draft atlas
//!   ([`crate::player::visuals::draft_atlas`], 256): the picture is at most 180
//!   points of a 256-pixel target, which the full 1,024 atlas cannot improve
//!   on and would cost a quarter more build and sixteen times the textures.
//! - **One build in flight for the stage, the latest selection wins.** A
//!   selection that changes while a build is out does not drop the task - a
//!   dropped wasm task does not cancel, so a second build would only run
//!   beside the first - and starts nothing; when the build lands, the stage
//!   asks for the newer body. This is the player's own discipline
//!   (`player::rigged::kick_rigged_builds`, one build per chassis).
//! - **The last body kept.** The body built last stays, hidden while
//!   something else is pictured, so picking it again builds nothing; one
//!   that landed for a selection already left is kept unspawned, and a body
//!   that failed to build is remembered as failed. Compared by the engine
//!   record while a wish is unmet (a build in flight, the frame between spawn
//!   and dress), never once a body stands.
//! - **Hung, dressed and posed as the game does it**: under a stand-in
//!   chassis by the game's root transform
//!   ([`crate::player::visuals::rigged_root_transform`]), dressed from
//!   [`crate::player::attachments::dressed_by`] through the game's seating
//!   ([`crate::player::attachments::placements`]) once the engine's joints
//!   exist, and standing in [`crate::player::visuals::standing_pose`] - the
//!   rest pose with the arms hung, no breath, no sway: a still picture. The
//!   stand-in chassis is turned so the face is seen, a person in three
//!   quarters and a beast more from the side ([`body_facing`]); a winged body
//!   as its posture goes, a bird as a person and a bat or a glider as a beast.
//! - **Framed on the posed body.** Bevy bounds a skinned mesh by its bind
//!   pose - the A-pose, arms out - so the skinned meshes' `Aabb`s are left out
//!   of the framing and the body's own [`crate::player::visuals::standing_box`]
//!   stands in for them, with what it wears added from the props' real
//!   bounds. A body is framed by its box, not by the sphere a tree is
//!   ([`body_placement`]): a person is tall and narrow, a beast long and low,
//!   and the sphere around either leaves most of the tile empty.
//!
//! A body that cannot be built - the engine's one failure, limbs overlapping
//! at a joint - stages nothing, and [`ItemPreview::failed`] says so for the
//! pane to put into words; the camera stays off.
//!
//! ## What the spawn path does NOT bring
//!
//! The subject is spawned with `avatar_mode` on and `is_local` off, which
//! is load-bearing: that mode strips colliders, the `RoomEntity` /
//! `PlacementUnit` cleanup tags and the `PrimMarker` gizmo key. A preview
//! is therefore not a physics body, not something a room rebuild sweeps,
//! and not something the gizmo can find. The one marker it does carry is
//! `AvatarVisualRoot`, which only the gait layer reads - and that layer
//! looks roots up *from* an entity that has a `GaitAnimation`, so a stage
//! with no such parent is never reached.
//!
//! A staged person, beast or winged body is held to the same: its stand-in chassis is no
//! physics body and carries no player or network marker, its root no
//! `RiggedRoot`, `Drive` or `AvatarDriver` - so the engine's motion driver,
//! the underfoot sampler, the gait and the nametags, which all find a body
//! through one of those, never reach it - and its worn props are spawned in
//! the same avatar mode as a tree, with no attachment markers. The layer
//! reaches the joints `spawn_avatar` makes and the props hung under them by
//! the same propagation.

use bevy::app::Propagate;
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy_egui::{EguiTextureHandle, EguiUserTextures};
use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarJoints, AvatarPose, spawn_avatar};

use bevy::ecs::change_detection::Tick;

use crate::offload::GenResult;
use crate::pds::avatar::{EngineAvatarRecord, ResolvedAttachment};
use crate::pds::{AvatarRecord, Generator, InventoryRecord};
use crate::player::attachments::{HatLine, dressed_by, ensure_joint_visibility, placements};
use crate::player::visuals::{
    AvatarSpawnDeps, draft_atlas, rigged_build_job, rigged_root_offset, rigged_root_transform,
    spawn_visual_tree, standing_box, standing_pose,
};
use crate::state::{AppState, LiveInventoryRecord};

/// The render layer the preview stage, its key light and its camera live
/// on. The world camera draws layer 0 - the layer every component-less
/// entity belongs to - which is the rest of the app but for the player's
/// body while it looks out of its own eyes
/// ([`crate::config::camera::FIRST_PERSON_BODY_LAYER`], #1745) and what the
/// agent's interface pictures hide.
pub const PREVIEW_LAYER: usize = 1;

/// Where the stage sits. Two kilometres up: far enough that a construct's
/// spatial audio is inaudible from the listener, near enough that `f32`
/// world coordinates there still resolve well under a millimetre.
const STAGE_ORIGIN: Vec3 = Vec3::new(0.0, 2_000.0, 0.0);

/// Edge of the square render target, in pixels. The detail panel draws it
/// at a smaller logical size, so this is the resolution the picture is
/// sampled *from* - 256 keeps it crisp on a 2x display without costing a
/// second full-size pass every frame the Catalogue is open.
const TARGET_SIZE: u32 = 256;

/// Backdrop behind the subject. Deliberately one fixed neutral rather than
/// a theme colour: the 3D pass runs outside egui and has no access to the
/// context the theme is read from, and a mid-slate reads as a backdrop
/// under both the light and the dark palettes.
const BACKDROP: Color = Color::srgb(0.14, 0.15, 0.19);

/// Yaw of the fixed three-quarter view, matching the angle the headless
/// `render` tool frames a single subject from.
const VIEW_YAW: f32 = std::f32::consts::FRAC_PI_4;
/// Elevation of that view, in degrees above the subject's centre.
const VIEW_ELEVATION_DEG: f32 = 18.0;
/// Vertical half-angle of the preview camera, and the margin the framing
/// leaves around the subject's bounding sphere.
const VIEW_FOV: f32 = std::f32::consts::FRAC_PI_4;
const FRAMING_MARGIN: f32 = 1.25;
/// How far round from a person's face the camera stands, in degrees: a
/// three-quarter view, the face and the build both read (#1842).
const PERSON_VIEW_DEG: f32 = 35.0;
/// The same for a beast: further round, so its length - what tells a horse
/// from a dog - is seen from the side, with the head still turned to the
/// camera.
const BEAST_VIEW_DEG: f32 = 60.0;
/// Floor on how close the camera will stand, in metres - five times its
/// own near plane, so a subject with no size cannot put geometry inside
/// it. Deliberately only just above the near plane: a larger floor is not
/// a safety margin, it is a rule that renders every small item small, and
/// the catalogue's smallest entries are centimetres across.
const MIN_VIEW_DISTANCE: f32 = 0.05;

/// What the UI is asking the preview to show (#1297, #1301).
///
/// `restage_preview` used to read [`crate::ui::toolbar::UiPanels`] and
/// [`crate::ui::catalogue::CatalogueBrowser`] itself, which pointed the
/// arrow from a render pipeline into the egui layer for one `bool` and
/// one `Option<&str>`. Written once a frame by
/// `ui::catalogue::mirror_preview_request` in `PreUpdate`, the
/// `world_builder::PlacementFocus` shape a second time - for the Inventory's
/// selection as well as the Catalogue's since #1301.
///
/// Still DERIVED rather than pushed: the mirror recomputes it from the
/// panel state through [`wanted_subject`], the same predicate the
/// consumer used to apply, so there is no request a panel draw could
/// dirty per frame (#879) and no second copy of the rule.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct PreviewRequest(pub Option<PreviewSubject>);

/// What the preview is showing, or is being asked to show. Compared by
/// value: a restage happens when - and only when - this differs from what
/// is already on the stage, so holding a selection costs one camera pass
/// per frame and no respawns.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PreviewSubject {
    /// A catalogue entry, by [`crate::catalogue::CatalogueEntry::slug`].
    Catalogue(String),
    /// An avatar in the Catalogue (#1840), by
    /// [`crate::catalogue::avatars::AvatarEntry::slug`]: a vehicle pictured
    /// from the visuals tree the entry builds, as a vehicle avatar item is,
    /// and a person, a beast or a winged body as its built body (#1842). Like a catalogue
    /// entry it is a recipe, so its picture never goes stale and it carries
    /// no tick.
    CatalogueAvatar(String),
    /// A stash item (#1301), by its key in [`InventoryRecord::generators`]
    /// or, for an avatar (#1838 a vehicle, #1842 a rigged body), in
    /// [`InventoryRecord::avatars`] - its NAME. The PDS rkey is derived from the name
    /// ([`crate::pds::inventory::item_rkey`]), so the name is the identity;
    /// a rename is a different subject.
    ///
    /// `edit` is [`LiveInventoryRecord`]'s change tick when the name was
    /// resolved, and it is what makes an edit restage the picture. The
    /// subject compares by value and a name does not change when the
    /// `Generator` under it does; comparing the `Generator` itself would
    /// cost a whole-tree `PartialEq` every frame, which is the per-frame
    /// work #1135/#1292 removed from the Inventory panel. The tick is the
    /// truth the panel's own dirty caches already key on, so any stash
    /// write restages the selected item ONCE - one respawn per discrete
    /// user action, and none while nothing is written (#1322).
    Inventory { name: String, edit: Tick },
}

/// The preview's render target, its egui handle, and what is currently on
/// the stage.
#[derive(Resource)]
pub struct ItemPreview {
    /// The off-screen image as egui sees it. Drop into
    /// `egui::Image::from_texture`; valid for the life of the app.
    pub egui_texture: bevy_egui::egui::TextureId,
    camera: Entity,
    /// Root of the spawned subject, despawned on the next restage.
    stage: Option<Entity>,
    /// What [`Self::stage`] holds. `None` means the camera is off.
    staged: Option<PreviewSubject>,
    /// Cleared on every restage and set once the framing has found real
    /// bounds - a freshly spawned tree has no `Aabb` until Bevy has
    /// computed one, so framing is a follow-up, not part of the spawn.
    framed: bool,
    /// The person, beast or winged body the staged subject wants standing
    /// (#1842), until it stands dressed - or turns out not to build. `None`
    /// for a tree.
    wants: Option<WantedBody>,
    /// The stage's one build in flight.
    build: Option<StageBuild>,
    /// The body built last, kept for a re-selection of it.
    kept: Option<KeptBody>,
    /// The staged subject's body could not be built ([`Self::failed`]).
    body_failed: bool,
}

impl ItemPreview {
    /// What the render target actually holds a usable picture OF.
    ///
    /// Deliberately narrower than "what is staged". A subject spawned this
    /// frame is staged immediately but not yet FRAMED - its meshes have no
    /// bounds until Bevy computes them, so the camera is still pointing
    /// where the previous subject was, and the picture drawn from it would
    /// be the new item seen from the old item's distance. Callers ask this
    /// question, never [`Self::staged`], so nothing can draw that frame.
    pub fn showing(&self) -> Option<&PreviewSubject> {
        self.staged.as_ref().filter(|_| self.framed)
    }

    /// What is on the stage, framed or not. The restage decision's own
    /// question; a drawing caller wants [`Self::showing`].
    fn staged(&self) -> Option<&PreviewSubject> {
        self.staged.as_ref()
    }

    /// The staged person, beast or winged body that could not be built
    /// (#1842), so there is no picture of it and never will be until the subject
    /// changes. The pane says so in place of a picture; `None` for anything
    /// else, a body still building included.
    pub fn failed(&self) -> Option<&PreviewSubject> {
        self.staged.as_ref().filter(|_| self.body_failed)
    }

    /// The kept body, when it is the one on show.
    fn shown_body(&self) -> Option<&StandingBody> {
        match &self.kept {
            Some(KeptBody {
                state: Kept::Standing(body),
                ..
            }) if body.shown => Some(body),
            _ => None,
        }
    }

    /// [`Self::shown_body`], to change.
    fn shown_body_mut(&mut self) -> Option<&mut StandingBody> {
        match &mut self.kept {
            Some(KeptBody {
                state: Kept::Standing(body),
                ..
            }) if body.shown => Some(body),
            _ => None,
        }
    }
}

/// A person, a beast or a winged body the stage wants standing (#1842): the engine record
/// to build, what to dress it in, and how far its chassis's centre sits
/// above its feet - all read off the avatar record the subject wears as.
struct WantedBody {
    record: EngineAvatarRecord,
    worn: Vec<ResolvedAttachment>,
    offset: f32,
}

impl WantedBody {
    /// What `record` stands, or `None` for a record that is not a resolved
    /// rigged body.
    fn of(record: &AvatarRecord) -> Option<Self> {
        let resolved = record.body.rigged_ref()?.resolved.as_ref()?;
        Some(Self {
            record: resolved.body.clone(),
            worn: dressed_by(record).unwrap_or_default().to_vec(),
            offset: rigged_root_offset(record),
        })
    }
}

/// The stage's one build in flight, and the record it builds.
struct StageBuild {
    record: EngineAvatarRecord,
    task: StageTask,
}

/// A build's task: the offloaded job, or - in a test - one the test
/// finishes by hand (see `held`).
enum StageTask {
    Offloaded(bevy::tasks::Task<GenResult>),
    #[cfg(test)]
    Held(std::sync::Arc<std::sync::Mutex<Option<GenResult>>>),
}

impl StageTask {
    /// The result, the first time it is ready.
    fn poll(&mut self) -> Option<GenResult> {
        match self {
            Self::Offloaded(task) => {
                bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(task))
            }
            #[cfg(test)]
            Self::Held(slot) => slot.lock().ok()?.take(),
        }
    }
}

/// The body built last, by the engine record it was built from.
struct KeptBody {
    record: EngineAvatarRecord,
    state: Kept,
}

/// Where the kept body is.
enum Kept {
    /// It could not be built: the engine's one failure.
    Failed,
    /// Built, and not spawned: it landed while another subject was wanted.
    /// With the head measure a web build brings (#1815).
    Built(Box<symbios_avatar::Avatar>, Option<HatLine>),
    /// Spawned on the stage, shown or hidden.
    Standing(StandingBody),
}

/// A body standing on the stage.
struct StandingBody {
    /// The stand-in chassis: the entity the layer propagates from, turned so
    /// the camera sees the face. Despawning it takes everything.
    stage: Entity,
    /// The root under it, hung by the game's root transform: where
    /// `spawn_avatar` hangs the joints and the meshes.
    root: Entity,
    /// What it is dressed in; `None` until it is (the joints arrive a
    /// command flush after the spawn) and while it is hidden.
    worn: Option<Vec<ResolvedAttachment>>,
    /// The worn props' own entities, each under its carrying joint.
    props: Vec<Entity>,
    /// The box the body fills standing, in the root's space.
    standing_box: (Vec3, Vec3),
    /// Whether it is the picture now, rather than kept out of sight.
    shown: bool,
    /// Frames the framing still waits once it is dressed: one, so the props
    /// spawned with the dressing have bounds and a layer before they are
    /// measured.
    settle: u8,
    /// The head measure the build took where the whole body was: a body
    /// from the web's worker, whose closed surface stayed behind (#1815).
    /// What the game's root carries as
    /// [`crate::player::attachments::CarriedHatLine`].
    hat_line: Option<HatLine>,
}

/// Marks the preview's own camera (#1300).
///
/// The stage is invisible to the world camera through [`PREVIEW_LAYER`],
/// but the CAMERA is a plain `Camera3d` and was therefore a second answer
/// to every `With<Camera3d>` query in the crate - see
/// [`crate::camera::WorldCamera`] for what that broke. This module
/// identifies its own camera positively for the same reason everything
/// else now identifies the player's.
#[derive(Component)]
pub struct PreviewCamera;

pub struct ItemPreviewPlugin;

impl Plugin for ItemPreviewPlugin {
    fn build(&self, app: &mut App) {
        // See the module header: the stage's descendants - including
        // particles spawned after the tree - get their layer from here,
        // in PostUpdate, which is before visibility is computed and
        // before the render world extracts.
        // The plugin only says WHICH schedule; ordering it before visibility
        // is what makes the claim above true. Without that the propagation
        // could land after the frame's visibility check and a freshly
        // spawned preview node - or a particle the emitter added this frame
        // - would be drawn by the WORLD camera once, two kilometres over the
        // player's head. Shared with the first-person camera (#1745), which
        // hides the player's body the same way: one registration, whichever
        // plugin asks first.
        crate::camera::register_render_layer_propagation(app);
        app.init_resource::<PreviewRequest>()
            .add_systems(Startup, setup_preview)
            .add_systems(
                Update,
                // The body stands before the sibling crate writes poses, so
                // a body spawned this frame is posed this frame (a command
                // flush separates the two), and its props hang from posed
                // joints when the framing measures them.
                (restage_preview, stand_staged_body)
                    .chain()
                    .before(bevy_symbios_avatar::AvatarSystems::Apply)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                PostUpdate,
                frame_preview.after(bevy::transform::TransformSystems::Propagate),
            )
            // Leaving the game does not close the Catalogue window, so
            // without this the stage and its camera pass would survive a
            // logout into the login screen - where the resources the
            // restage needs are gone and it cannot clean up after itself.
            .add_systems(OnExit(AppState::InGame), clear_preview);
    }
}

/// Create the render target, hand it to egui, and spawn the camera and key
/// light that will look at whatever the stage holds. Runs once; the camera
/// starts inactive because nothing is selected yet.
///
/// `pub(crate)` for `crate::camera`'s #1317 test, which boots both camera
/// spawns in either order and checks who ends up with the egui context.
pub(crate) fn setup_preview(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut egui_textures: ResMut<EguiUserTextures>,
) {
    let image = images.add(new_target());
    // A strong handle, exactly as the profile-picture cache does it
    // (`crate::avatar`): egui then holds the asset alive independently of
    // this resource, and there is nothing to evict because there is only
    // ever one target.
    let egui_texture = egui_textures.add_image(EguiTextureHandle::Strong(image.clone()));

    let layers = RenderLayers::layer(PREVIEW_LAYER);
    let camera = commands
        .spawn((
            Camera3d::default(),
            crate::camera::TONEMAPPING,
            PreviewCamera,
            // Deliberately NO egui context. bevy_egui's automatic pick is
            // off (#1317, `crate::camera::egui_global_settings`), so a
            // camera gets one only by naming `PrimaryEguiContext`, and this
            // one never should: egui drawing through an inactive 256 px
            // off-screen camera is exactly what took the login screen off
            // the wasm build.
            Camera {
                // Ahead of the main camera, so the texture egui samples
                // this frame was drawn this frame.
                order: -1,
                clear_color: ClearColorConfig::Custom(BACKDROP),
                // Off until something is selected. A preview nobody is
                // looking at must not cost a render pass - this is the
                // whole reason the camera is spawned once and toggled
                // rather than spawned per selection.
                is_active: false,
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            // The WebGL2 build panics on Bevy's default `Msaa::Sample4`,
            // the reason its main camera draws FXAA in its place (see
            // `crate::camera::EdgeSmoothing`).
            Msaa::Off,
            Projection::from(PerspectiveProjection {
                fov: VIEW_FOV,
                // The stage holds one item and nothing else, so the depth
                // range only has to cover it.
                near: 0.01,
                far: 200.0,
                ..default()
            }),
            // `GlobalAmbientLight` is a resource in this Bevy and would
            // otherwise light the preview with whatever the current room's
            // sky is doing. A per-camera `AmbientLight` overrides it, so
            // the same item looks the same at midnight and at noon.
            AmbientLight {
                color: Color::WHITE,
                brightness: 600.0,
                ..default()
            },
            layers.clone(),
            Transform::from_translation(STAGE_ORIGIN + Vec3::new(0.0, 0.5, 2.0))
                .looking_at(STAGE_ORIGIN, Vec3::Y),
        ))
        .id();

    // A POINT light, not a directional one: a `DirectionalLight` is
    // position-independent, so even confined to this layer it would want
    // shadow cascades sized for a world. Shadows are off outright - a
    // 256 px thumbnail cannot show them and they would double the pass.
    commands.spawn((
        PointLight {
            intensity: 4_000_000.0,
            range: 40.0,
            shadow_maps_enabled: false,
            ..default()
        },
        layers,
        Transform::from_translation(STAGE_ORIGIN + Vec3::new(3.0, 4.0, 4.0)),
    ));

    // The `Image` handle itself is deliberately NOT kept here: egui holds
    // a strong handle to it and so does the camera's `RenderTarget`, so a
    // third copy in this resource would only be a field nothing reads.
    commands.insert_resource(ItemPreview {
        egui_texture,
        camera,
        stage: None,
        staged: None,
        framed: false,
        wants: None,
        build: None,
        kept: None,
        body_failed: false,
    });
}

/// Take everything down: despawn the stage and switch the camera off.
/// Idempotent, so it is safe on a state exit that never had a preview.
fn clear_preview(
    mut commands: Commands,
    mut preview: ResMut<ItemPreview>,
    mut cameras: Query<&mut Camera, With<PreviewCamera>>,
) {
    if let Some(stage) = preview.stage.take() {
        commands.entity(stage).despawn();
    }
    preview.staged = None;
    preview.framed = false;
    preview.wants = None;
    preview.body_failed = false;
    // The kept body goes too: the login screen has no use for its meshes.
    // A build still in flight is NOT dropped - on the web that would not
    // stop it, only lose the one handle that says it is out - and lands as
    // the kept body when the stage next runs.
    if let Some(KeptBody {
        state: Kept::Standing(body),
        ..
    }) = preview.kept.take()
    {
        commands.entity(body.stage).try_despawn();
    }
    if let Ok(mut camera) = cameras.get_mut(preview.camera) {
        camera.is_active = false;
    }
}

/// One window's bid for the stage (#1301): whether the window is open,
/// what it has selected, and when that was picked, in
/// `Time::elapsed_secs_f64` seconds.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Pick<'a> {
    pub(crate) open: bool,
    pub(crate) selected: Option<&'a str>,
    pub(crate) at: f64,
}

/// The subject the UI is asking for. Derived rather than pushed - there is
/// no request resource for a panel to write every frame, so there is no
/// change-tick to guard (#879).
///
/// Each window's pick resolves on its own ([`catalogue_subject`],
/// [`inventory_subject`]); then the most RECENT one that resolves wins
/// (#1301). There is one stage, and "most recent" is the only order a user
/// can see: the other window keeps its selection and its facts and shows
/// the placeholder tile, where clearing its selection instead would take
/// away a pick the user never touched. A pick that cannot be pictured - a
/// room-scoped stash item, a name that no longer resolves - does not take
/// the stage from one that can. On an exact tie the Catalogue keeps it,
/// which is only a determinism rule: one frame cannot hold two clicks.
///
/// `stash` is the live stash and its change tick, `None` before it loads.
pub(crate) fn wanted_subject(
    catalogue: Pick<'_>,
    inventory: Pick<'_>,
    stash: Option<(&InventoryRecord, Tick)>,
) -> Option<PreviewSubject> {
    let from_catalogue = catalogue_subject(catalogue);
    let from_inventory = inventory_subject(inventory, stash);
    match (from_catalogue, from_inventory) {
        (Some(c), Some(i)) => Some(if inventory.at > catalogue.at { i } else { c }),
        (c, i) => c.or(i),
    }
}

/// The Catalogue's bid: its selected entry, while its window is open.
fn catalogue_subject(pick: Pick<'_>) -> Option<PreviewSubject> {
    // A closed window is the whole "is anyone looking" question: it is what
    // turns the camera off, and a camera pass nobody can see is the one
    // cost this approach could have carried and does not.
    if !pick.open {
        return None;
    }
    let slug = pick.selected?;
    // A selection that no longer resolves shows nothing rather than the
    // last thing that did.
    if crate::catalogue::by_slug(slug).is_some() {
        return Some(PreviewSubject::Catalogue(slug.to_string()));
    }
    // An avatar entry (#1840) is asked for by its slug alone, which builds
    // nothing: a vehicle has a tree to stage, and a person, a beast or a
    // winged body has a body to build (#1842). The restage builds the entry, once.
    // A world entry (#1841) is never asked for: it is a text card, and a
    // world is never built to be looked at.
    crate::catalogue::avatars::avatar_by_slug(slug)
        .map(|entry| PreviewSubject::CatalogueAvatar(entry.slug.to_string()))
}

/// The Inventory's bid (#1301): its selected item, while its window is
/// open, the stash has loaded, the name still resolves, and the item is
/// something the stage can hold - a generator item, an avatar item's
/// visuals tree ([`InventoryRecord::picture_of`], #1838), or a person or a
/// beast to build ([`InventoryRecord::body_picture_of`], #1842).
///
/// What a TREE on the stage must be is [`crate::pds::inventory::is_drop_placeable`]
/// (which [`InventoryRecord::picture_of`] applies), and it
/// is not a nicety. The spawn path runs `avatar_mode`, but the Terrain arm
/// of `spawn_generator` tags its anchor `RoomEntity` regardless, and a
/// Water child spawns a volume the width of the whole region and registers
/// it in `WaterSurfaces` - a room-scoped item on the stage would reach into
/// the world. An unreadable item (`GeneratorKind::Unknown`) has nothing to
/// draw. The pane says why in each case; the stage stays out of it.
fn inventory_subject(
    pick: Pick<'_>,
    stash: Option<(&InventoryRecord, Tick)>,
) -> Option<PreviewSubject> {
    if !pick.open {
        return None;
    }
    let name = pick.selected?;
    let (record, edit) = stash?;
    // An avatar item (#1838) is pictured from its visuals tree when it has
    // one, and from its built body when it is a person, a beast or a winged
    // body (#1842).
    if record.picture_of(name).is_none() && record.body_picture_of(name).is_none() {
        return None;
    }
    Some(PreviewSubject::Inventory {
        name: name.to_string(),
        edit,
    })
}

// How many times `restage_preview` swapped the stage, for the #1301 work
// count in `ui::catalogue`'s tests - the instrument `NODES_BUILT` is for the
// generator tree. Thread-local, not global: `cargo test --lib` runs the
// suite on many threads of one process (#1147, #1189).
#[cfg(test)]
thread_local! {
    static RESTAGES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Restages counted on this thread since the last call. Test-only.
#[cfg(test)]
pub(crate) fn take_restage_count() -> usize {
    RESTAGES.with(|n| n.replace(0))
}

/// Swap the stage over when the selection changes, and switch the camera
/// off when there is nothing selected.
///
/// `pub(crate)` for `ui::catalogue`'s #1301 work count, which runs it
/// behind the real mirror.
#[allow(clippy::too_many_arguments)]
pub(crate) fn restage_preview(
    mut commands: Commands,
    mut preview: ResMut<ItemPreview>,
    request: Res<PreviewRequest>,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    // Where an Inventory subject's `Generator` lives (#1301). Read only on
    // a restage, never compared: the subject's `edit` tick already said
    // whether it moved.
    stash: Option<Res<LiveInventoryRecord>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
    mut cameras: Query<&mut Camera, With<PreviewCamera>>,
) {
    let wanted = request.0.clone();
    if wanted.as_ref() == preview.staged() {
        return;
    }
    #[cfg(test)]
    RESTAGES.with(|n| n.set(n.get() + 1));

    if let Some(stage) = preview.stage.take() {
        commands.entity(stage).despawn();
    }
    preview.framed = false;
    // Whatever the new subject is, the last one's body goes out of sight and
    // its wish is withdrawn: a build it started stays in flight and lands
    // as the kept body (see the module header).
    preview.wants = None;
    preview.body_failed = false;
    hide_kept_body(&mut commands, &mut preview);

    let Some(subject) = wanted else {
        preview.staged = None;
        if let Ok(mut camera) = cameras.get_mut(preview.camera) {
            camera.is_active = false;
        }
        return;
    };

    let did = session.as_ref().map(|s| s.did.as_str()).unwrap_or("");
    let generator = match staging_for(&subject, did, stash.as_deref().map(|stash| &stash.0)) {
        Some(Staging::Tree(generator)) => generator,
        Some(Staging::Body(record)) => {
            // A person or a beast (#1842): nothing to spawn yet. The wish is
            // what `stand_staged_body` builds, stands and dresses; the camera
            // stays off until there is a body to look at, so a build in
            // flight costs no passes.
            let Some(wanted) = WantedBody::of(&record) else {
                preview.staged = None;
                if let Ok(mut camera) = cameras.get_mut(preview.camera) {
                    camera.is_active = false;
                }
                return;
            };
            preview.wants = Some(wanted);
            preview.staged = Some(subject);
            if let Ok(mut camera) = cameras.get_mut(preview.camera) {
                camera.is_active = false;
            }
            return;
        }
        None => {
            preview.staged = None;
            return;
        }
    };

    let stage = commands
        .spawn((
            Transform::from_translation(STAGE_ORIGIN),
            Visibility::default(),
            // Propagated, not inserted: see the module header.
            Propagate(RenderLayers::layer(PREVIEW_LAYER)),
        ))
        .id();
    spawn_visual_tree(
        &mut commands,
        stage,
        &generator,
        &mut meshes,
        &mut materials,
        &mut images,
        &mut deps,
        // Not the local avatar: this must not carry the visuals-node gizmo
        // marker, which is what makes a node selectable in the editor.
        false,
    );

    preview.stage = Some(stage);
    preview.staged = Some(subject);
    if let Ok(mut camera) = cameras.get_mut(preview.camera) {
        camera.is_active = true;
    }
}

/// What a subject is pictured from: a tree the spawn path stages at once,
/// or the avatar record a person or a beast wears as, whose body is built.
///
/// One lives for the length of a restage, on the stack; boxing the owned
/// tree to even the variants out would only add an allocation per restage.
#[allow(clippy::large_enum_variant)]
enum Staging<'a> {
    Tree(std::borrow::Cow<'a, Generator>),
    Body(std::borrow::Cow<'a, AvatarRecord>),
}

/// What `subject` is pictured from, or `None` when it no longer resolves to
/// anything the stage may hold. `did` is the session's, `stash` the live
/// stash.
///
/// A catalogue entry is BUILT, so it is owned; a stash item is borrowed
/// from the record for the length of the spawn, so a restage costs no copy
/// of a tree that may be the full 100 KiB record budget. A stash person is
/// the one owned record a stash item gives: the avatar record it would be
/// worn as ([`crate::pds::inventory::AvatarItem::wear`]), which is what the
/// game reads a body and its wearables from.
fn staging_for<'a>(
    subject: &PreviewSubject,
    did: &str,
    stash: Option<&'a InventoryRecord>,
) -> Option<Staging<'a>> {
    use std::borrow::Cow;
    match subject {
        // The DID an entry builds against personalises a handful of items (a
        // gateway's plaque, a monument's picture). Signed out, an empty DID
        // is what the placement-check path already passes.
        PreviewSubject::Catalogue(slug) => {
            crate::catalogue::by_slug(slug).map(|entry| Staging::Tree(Cow::Owned(entry.build(did))))
        }
        PreviewSubject::CatalogueAvatar(slug) => {
            let entry = crate::catalogue::avatars::avatar_by_slug(slug)?;
            if entry.is_rigged() {
                // Built on the restage - the entry's own cache keeps it.
                return entry
                    .record()
                    .map(|record| Staging::Body(Cow::Borrowed(record)));
            }
            // Borrowed from the entry's cache, held to the stash's rule for
            // what the stage may hold.
            entry
                .visuals()
                .filter(|generator| crate::pds::inventory::is_drop_placeable(generator))
                .map(|generator| Staging::Tree(Cow::Borrowed(generator)))
        }
        PreviewSubject::Inventory { name, .. } => {
            // `wanted_subject` already refused anything else; this is the
            // same rule held where the spawn happens, because a request is a
            // frame old by the time it is read here.
            let stash = stash?;
            if let Some(generator) = stash.picture_of(name) {
                return Some(Staging::Tree(Cow::Borrowed(generator)));
            }
            stash
                .body_picture_of(name)?
                // The keys a worn copy would be saved under mean nothing on
                // the stage, which neither saves nor edits it.
                .wear(|_| String::new(), String::new)
                .map(|record| Staging::Body(Cow::Owned(record)))
        }
    }
}

/// Take the kept body out of sight, and its props off it: what a restage
/// does to it before anything else is staged. Kept, not despawned, so picking
/// it again builds nothing; undressed, because the props are cheap to hang
/// again and a hidden body's would only be bounds the framing has to skip.
fn hide_kept_body(commands: &mut Commands, preview: &mut ItemPreview) {
    let Some(body) = preview.shown_body_mut() else {
        return;
    };
    body.shown = false;
    body.worn = None;
    for prop in body.props.drain(..) {
        commands.entity(prop).try_despawn();
    }
    commands.entity(body.stage).try_insert(Visibility::Hidden);
}

// The builds the stage kicked on this thread, for the work counts in the
// tests below - `RESTAGES`'s neighbour.
#[cfg(test)]
thread_local! {
    static KICKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Builds kicked on this thread since the last call. Test-only.
#[cfg(test)]
pub(crate) fn take_build_kicks() -> usize {
    KICKS.with(|n| n.replace(0))
}

/// Start the stage's build of `record`: the game's own job at the draft
/// atlas, offloaded - on the web, to the worker.
fn kick_body_build(record: &EngineAvatarRecord) -> StageBuild {
    #[cfg(test)]
    KICKS.with(|n| n.set(n.get() + 1));
    let job = rigged_build_job(record, draft_atlas());
    #[cfg(test)]
    if let Some(task) = held::hold(&job) {
        return StageBuild {
            record: record.clone(),
            task,
        };
    }
    StageBuild {
        record: record.clone(),
        task: StageTask::Offloaded(crate::offload::offload(job)),
    }
}

/// Bring the staged person or beast (#1842) onto the stage: land the build
/// in flight, kick the next one, stand a built body and dress it.
///
/// Every step happens once per subject - but for one record comparison a
/// frame while a wish is unmet: with nothing wanted and nothing in flight
/// the system returns before it touches anything, and a wish is dropped the
/// frame it is met. In order:
///
/// 1. **Land.** A finished build becomes the kept body - unless the kept
///    body is the one on show, which a re-selection brought back while the
///    build was out: then the build is the stale one, and goes.
/// 2. **Build.** A wish the kept body does not answer kicks a build, unless
///    one is already in flight: it lands first, and step 1 of a later frame
///    finds the wish still unanswered.
/// 3. **Stand.** A kept body that answers the wish is spawned if it is not
///    standing yet, shown if it is, and the camera goes on.
/// 4. **Dress.** Once its joints exist - a command flush after the spawn -
///    it is dressed in what the wish wears, unless it already is; the wish
///    is then met. A body that failed is met at once, as a failure.
#[allow(clippy::too_many_arguments)]
fn stand_staged_body(
    mut commands: Commands,
    mut preview: ResMut<ItemPreview>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut deps: AvatarSpawnDeps,
    bodies: Query<(&BuiltBody, &AvatarJoints)>,
    mut cameras: Query<&mut Camera, With<PreviewCamera>>,
) {
    // Read through `Deref` first: an idle stage must not stamp the
    // resource's change tick every frame.
    if preview.build.is_none() && preview.wants.is_none() {
        return;
    }
    let preview = preview.as_mut();

    // 1. Land.
    if let Some(result) = preview.build.as_mut().and_then(|build| build.task.poll())
        && let Some(build) = preview.build.take()
    {
        let (avatar, hat_line) = match result {
            GenResult::Avatar(avatar) => (avatar, None),
            GenResult::MeasuredAvatar { body, hat_line } => (body, hat_line),
            _ => {
                error!("the item preview's body build returned another job's result");
                (None, None)
            }
        };
        if avatar.is_none() {
            warn!("the item preview was asked to picture a body that could not be built");
        }
        if preview.shown_body().is_none() {
            if let Some(KeptBody {
                state: Kept::Standing(old),
                ..
            }) = preview.kept.take()
            {
                commands.entity(old.stage).try_despawn();
            }
            preview.kept = Some(KeptBody {
                record: build.record,
                state: avatar.map_or(Kept::Failed, |avatar| Kept::Built(avatar, hat_line)),
            });
        }
    }

    // 2. Build.
    let Some(wants) = preview.wants.as_ref() else {
        return;
    };
    let answered = preview
        .kept
        .as_ref()
        .is_some_and(|kept| kept.record == wants.record);
    if !answered {
        if preview.build.is_none() {
            preview.build = Some(kick_body_build(&wants.record));
        }
        return;
    }

    // 3. Stand.
    let Some(kept) = preview.kept.as_mut() else {
        return;
    };
    if matches!(kept.state, Kept::Failed) {
        preview.wants = None;
        preview.body_failed = true;
        if let Ok(mut camera) = cameras.get_mut(preview.camera) {
            camera.is_active = false;
        }
        return;
    }
    if matches!(kept.state, Kept::Built(..))
        && let Kept::Built(avatar, hat_line) = std::mem::replace(&mut kept.state, Kept::Failed)
    {
        kept.state = Kept::Standing(stand_body(
            &mut commands,
            *avatar,
            hat_line,
            &kept.record,
            wants.offset,
            &mut meshes,
            &mut materials,
            &mut images,
            &mut bindposes,
        ));
    }
    let Kept::Standing(body) = &mut kept.state else {
        return;
    };
    if !body.shown {
        body.shown = true;
        commands
            .entity(body.stage)
            .try_insert(Visibility::Inherited);
    }
    if let Ok(mut camera) = cameras.get_mut(preview.camera)
        && !camera.is_active
    {
        camera.is_active = true;
    }

    // 4. Dress.
    let Ok((built, joints)) = bodies.get(body.root) else {
        return;
    };
    if body.worn.as_ref() != Some(&wants.worn) {
        for prop in body.props.drain(..) {
            commands.entity(prop).try_despawn();
        }
        ensure_joint_visibility(&mut commands, joints);
        for (joint, transform, attachment) in placements(&built.avatar, body.hat_line, &wants.worn)
        {
            let Some(&carrier) = joints.0.get(joint) else {
                continue;
            };
            // No `AttachmentRoot`, no `LocalAttachment`: those are what the
            // player's own dressing and the editor find a worn prop by, and
            // this one is nobody's.
            let prop = commands
                .spawn((transform, Visibility::default(), ChildOf(carrier)))
                .id();
            spawn_visual_tree(
                &mut commands,
                prop,
                &attachment.record.item,
                &mut meshes,
                &mut materials,
                &mut images,
                &mut deps,
                false,
            );
            body.props.push(prop);
        }
        body.worn = Some(wants.worn.clone());
    }
    body.settle = 1;
    preview.wants = None;
}

/// Spawn `avatar` on the stage as the game hangs a body under a chassis: a
/// stand-in chassis whose centre is `offset` above the stage floor, the root
/// under it by the game's own [`rigged_root_transform`], the body in the
/// [`standing_pose`]. The chassis carries the layer and the turn that shows
/// the face ([`body_facing`]); nothing on it is a physics body or a marker
/// any world system reads.
#[allow(clippy::too_many_arguments)]
fn stand_body(
    commands: &mut Commands,
    avatar: symbios_avatar::Avatar,
    hat_line: Option<HatLine>,
    record: &EngineAvatarRecord,
    offset: f32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) -> StandingBody {
    let beast = shows_its_flank(record);
    let standing_box = standing_box(&avatar);
    let stage = commands
        .spawn((
            Transform::from_translation(STAGE_ORIGIN + Vec3::Y * offset)
                .with_rotation(body_facing(beast)),
            Visibility::default(),
            Propagate(RenderLayers::layer(PREVIEW_LAYER)),
        ))
        .id();
    let root = commands
        .spawn((
            rigged_root_transform(offset),
            Visibility::default(),
            AvatarPose(standing_pose(&avatar.rig)),
            ChildOf(stage),
        ))
        .id();
    spawn_avatar(
        commands, root, avatar, 0.0, meshes, materials, images, bindposes,
    );
    StandingBody {
        stage,
        root,
        worn: None,
        props: Vec::new(),
        standing_box,
        shown: true,
        settle: 1,
        hat_line,
    }
}

/// Whether `record`'s body is turned as a beast is ([`body_facing`]), its
/// flank to the camera: a body that goes on four legs, which is a beast and a
/// winged body a bat or a glider leads (#1875). A bird stands up as a person
/// does, and is turned to show its face.
fn shows_its_flank(record: &EngineAvatarRecord) -> bool {
    crate::pds::avatar::RiggedKind::of_record(record).on_four()
}

/// How the stand-in chassis turns a body so the camera sees it: a person's
/// face [`PERSON_VIEW_DEG`] off the camera's line, a beast's nose
/// [`BEAST_VIEW_DEG`] off it.
///
/// Under the game's root transform the body faces the chassis's `-Z`, whose
/// yaw is a half turn; the camera stands at [`VIEW_YAW`]. So the chassis turns
/// by the camera's yaw, less the half turn, less the angle round from the
/// face it should see.
fn body_facing(beast: bool) -> Quat {
    let round = if beast {
        BEAST_VIEW_DEG
    } else {
        PERSON_VIEW_DEG
    };
    Quat::from_rotation_y(VIEW_YAW - round.to_radians() - std::f32::consts::PI)
}

/// An [`ItemPreview`] over `camera` with nothing staged, for tests that run
/// [`restage_preview`] without the `Startup` system - which needs a render
/// target and egui's texture registry.
#[cfg(test)]
pub(crate) fn preview_for_test(camera: Entity) -> ItemPreview {
    ItemPreview {
        egui_texture: bevy_egui::egui::TextureId::User(0),
        camera,
        stage: None,
        staged: None,
        framed: false,
        wants: None,
        build: None,
        kept: None,
        body_failed: false,
    }
}

/// Point the camera at the staged subject once its bounds exist.
///
/// A tree spawned this frame has no `Aabb` yet - Bevy computes one when
/// the mesh asset resolves - so the framing cannot be part of the spawn.
/// It retries every frame until it finds geometry, then latches: the
/// camera must not drift while a particle plume changes the bounds
/// underneath it.
///
/// A person or a beast (#1842) is framed once it stands dressed and a frame
/// has passed for its props to be measured: on the box its posed body fills
/// ([`StandingBody::standing_box`]), carried to the world by its root, and
/// the props' own bounds. Never on its skinned meshes' `Aabb`s, which are
/// the bind pose's - see the module header.
fn frame_preview(
    mut preview: ResMut<ItemPreview>,
    mut cameras: Query<&mut Transform, With<PreviewCamera>>,
    bounds: Query<(&GlobalTransform, &Aabb, &RenderLayers), Without<SkinnedMesh>>,
    globals: Query<&GlobalTransform>,
) {
    // Read through `Deref`: a framed or empty stage stamps nothing.
    if preview.framed || (preview.stage.is_none() && preview.shown_body().is_none()) {
        return;
    }
    let layers = RenderLayers::layer(PREVIEW_LAYER);
    let placement = if preview.stage.is_some() {
        let Some((centre, radius)) = stage_bounds(&bounds, &layers) else {
            return;
        };
        camera_placement(centre, radius)
    } else {
        let Some(body) = preview.shown_body_mut() else {
            return;
        };
        if body.worn.is_none() {
            return;
        }
        if body.settle > 0 {
            body.settle -= 1;
            return;
        }
        let Ok(root) = globals.get(body.root) else {
            return;
        };
        // The turned body's own corners, not the world box around them: the
        // stand-in chassis turns a body a third of the way round, and the
        // axis-aligned box of a turned beast is half as large again.
        let (min, max) = body.standing_box;
        let mut points: Vec<Vec3> = box_corners(min, max)
            .map(|corner| root.transform_point(corner))
            .collect();
        if let Some((props_min, props_max)) = stage_box(&bounds, &layers) {
            points.extend(box_corners(props_min, props_max));
        }
        body_placement(&points)
    };
    let Ok(mut transform) = cameras.get_mut(preview.camera) else {
        return;
    };
    *transform = placement;
    preview.framed = true;
}

/// Where the camera stands to see a bounding sphere of `radius` at
/// `centre`: the fixed three-quarter view, backed off far enough that the
/// sphere fits the vertical field of view with [`FRAMING_MARGIN`] to
/// spare.
///
/// [`MIN_VIEW_DISTANCE`] is what keeps a degenerate subject - a flat sign, a
/// single particle anchor with no geometry around it - from putting the
/// near plane inside itself.
fn camera_placement(centre: Vec3, radius: f32) -> Transform {
    let distance = (radius / (VIEW_FOV * 0.5).sin() * FRAMING_MARGIN).max(MIN_VIEW_DISTANCE);
    Transform::from_translation(centre + view_direction() * distance).looking_at(centre, Vec3::Y)
}

/// The unit vector from a subject's centre to the camera: the fixed
/// three-quarter view's yaw and elevation.
fn view_direction() -> Vec3 {
    let elevation = VIEW_ELEVATION_DEG.to_radians();
    Vec3::new(
        elevation.cos() * VIEW_YAW.sin(),
        elevation.sin(),
        elevation.cos() * VIEW_YAW.cos(),
    )
}

/// Where the camera stands to see `points` - the corners of a standing
/// body's box and of what it wears (#1842) - from the same view a tree is
/// seen from, aimed at the middle of them and backed off until every one
/// fits the square field of view with [`FRAMING_MARGIN`] to spare.
///
/// The points' own fit, not their bounding sphere's: a person is tall and
/// narrow and a beast long and low, and the sphere around either is mostly
/// air. A point nearer the camera than the middle needs more room, which is
/// the `offset.dot(back)` term.
fn body_placement(points: &[Vec3]) -> Transform {
    let (min, max) = points.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), &p| (min.min(p), max.max(p)),
    );
    let centre = (min + max) * 0.5;
    let back = view_direction();
    let right = Vec3::Y.cross(back).normalize();
    let up = back.cross(right);
    let reach = (VIEW_FOV * 0.5).tan() / FRAMING_MARGIN;
    let distance = points
        .iter()
        .map(|&point| {
            let offset = point - centre;
            let across = offset.dot(right).abs().max(offset.dot(up).abs());
            offset.dot(back) + across / reach
        })
        .fold(MIN_VIEW_DISTANCE, f32::max);
    Transform::from_translation(centre + back * distance).looking_at(centre, Vec3::Y)
}

/// The eight corners of the box `(min, max)`.
fn box_corners(min: Vec3, max: Vec3) -> impl Iterator<Item = Vec3> {
    (0..8).map(move |i| {
        Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        )
    })
}

/// The world box around `(min, max)` in the space `transform` carries to
/// the world.
fn transformed_box(transform: &GlobalTransform, (min, max): (Vec3, Vec3)) -> (Vec3, Vec3) {
    box_corners(min, max)
        .map(|corner| transform.transform_point(corner))
        .fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), p| (min.min(p), max.max(p)),
        )
}

/// Union of the world-space bounds of everything on the preview layer →
/// `(centre, bounding radius)`, or `None` while the stage has no geometry.
///
/// Reading the layer rather than walking the hierarchy is what keeps this
/// honest about particles: an emitter's children are separate entities
/// that the propagation has already reached, and a hierarchy walk from the
/// stage root would have to re-derive that relationship.
fn stage_bounds(
    bounds: &Query<(&GlobalTransform, &Aabb, &RenderLayers), Without<SkinnedMesh>>,
    layers: &RenderLayers,
) -> Option<(Vec3, f32)> {
    let (min, max) = stage_box(bounds, layers)?;
    Some(((min + max) * 0.5, ((max - min) * 0.5).length().max(0.05)))
}

/// [`stage_bounds`]' box: the union of every unskinned `Aabb` on the layer,
/// in world space. A skinned mesh is left out - its `Aabb` is its bind pose's
/// (#1842) - which no tree has, so for a tree this is every bound.
fn stage_box(
    bounds: &Query<(&GlobalTransform, &Aabb, &RenderLayers), Without<SkinnedMesh>>,
    layers: &RenderLayers,
) -> Option<(Vec3, Vec3)> {
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let mut any = false;
    for (transform, aabb, entity_layers) in bounds.iter() {
        if !entity_layers.intersects(layers) {
            continue;
        }
        any = true;
        let centre = Vec3::from(aabb.center);
        let half = Vec3::from(aabb.half_extents);
        let (aabb_min, aabb_max) = transformed_box(transform, (centre - half, centre + half));
        min = min.min(aabb_min);
        max = max.max(aabb_max);
    }
    any.then_some((min, max))
}

/// The off-screen colour target.
///
/// `RenderAssetUsages::RENDER_WORLD` and nothing else: the main-world CPU
/// copy is what #565 measured as the dominant retention on wasm, and this
/// image is only ever sampled by egui on the GPU. `TEXTURE_BINDING` is
/// what makes that sampling legal; there is no `COPY_SRC` because nothing
/// reads it back.
fn new_target() -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: TARGET_SIZE,
            height: TARGET_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING;
    image
}

/// Builds a test finishes by hand (#1842): while a test holds them, the
/// stage's build is not offloaded but parked here, so the test decides when
/// it lands and with what - a real body it builds on its own thread from the
/// very job the stage asked for, or the engine's one failure. Never the
/// shared compute pool, which a single `cargo test` process shares with
/// every other test's builds (#1295, #1501).
#[cfg(test)]
pub(crate) mod held {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex};

    use super::{GenResult, StageTask};
    use crate::offload::GenJob;

    thread_local! {
        static HOLDING: RefCell<Option<Vec<HeldBuild>>> = const { RefCell::new(None) };
    }

    /// A build the stage kicked: the job it asked for, and the slot its
    /// result lands through.
    pub(crate) struct HeldBuild {
        pub(crate) job: GenJob,
        slot: Arc<Mutex<Option<GenResult>>>,
    }

    impl HeldBuild {
        /// Land `result`, on the stage's next poll.
        pub(crate) fn finish(self, result: GenResult) {
            *self.slot.lock().expect("the slot") = Some(result);
        }

        /// Run the job the stage asked for - here, on the test's thread - and
        /// land what it built.
        pub(crate) fn build(self) {
            let result = self.job.clone().run();
            self.finish(result);
        }
    }

    /// Park every build the stage kicks on this thread from now on.
    pub(crate) fn hold_builds() {
        HOLDING.with(|holding| *holding.borrow_mut() = Some(Vec::new()));
    }

    /// The builds parked since the last call, oldest first.
    pub(crate) fn take_held() -> Vec<HeldBuild> {
        HOLDING.with(|holding| {
            holding
                .borrow_mut()
                .as_mut()
                .map(std::mem::take)
                .unwrap_or_default()
        })
    }

    /// Park `job`, when this thread holds builds.
    pub(super) fn hold(job: &GenJob) -> Option<StageTask> {
        HOLDING.with(|holding| {
            let mut holding = holding.borrow_mut();
            let parked = holding.as_mut()?;
            let slot = Arc::new(Mutex::new(None));
            parked.push(HeldBuild {
                job: job.clone(),
                slot: slot.clone(),
            });
            Some(StageTask::Held(slot))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Any real slug, so the tests below say something about the actual
    /// catalogue rather than about a string literal.
    fn a_real_slug() -> &'static str {
        crate::catalogue::ENTRIES[0].slug()
    }

    /// The Catalogue's bid alone, with the Inventory shut and no stash -
    /// the whole of `wanted_subject`'s input before #1301.
    fn catalogue_only(open: bool, selected: Option<&str>) -> Option<PreviewSubject> {
        wanted_subject(
            Pick {
                open,
                selected,
                at: 0.0,
            },
            Pick::default(),
            None,
        )
    }

    fn preview_showing(subject: Option<PreviewSubject>, framed: bool) -> ItemPreview {
        ItemPreview {
            staged: subject,
            framed,
            ..preview_for_test(Entity::PLACEHOLDER)
        }
    }

    /// #1288. The gate that decides whether a render pass happens at all.
    /// A closed Catalogue is the "nobody is looking" answer, and it is the
    /// only thing standing between this feature and a camera that draws a
    /// 256 px texture every frame for the whole session.
    #[test]
    fn nothing_is_previewed_while_the_catalogue_is_closed() {
        assert_eq!(catalogue_only(false, Some(a_real_slug())), None);
        assert_eq!(
            catalogue_only(true, Some(a_real_slug())),
            Some(PreviewSubject::Catalogue(a_real_slug().to_string())),
            "an open window over a real selection is the one case that renders"
        );
        assert_eq!(catalogue_only(true, None), None, "nothing selected");
    }

    /// #1288. A slug the catalogue no longer answers to shows NOTHING,
    /// not whatever was last on the stage. The Catalogue's selection is a
    /// `String` held across frames while `ENTRIES` is a compile-time list,
    /// so the two can disagree the moment an entry is renamed or dropped
    /// -- and the failure mode of trusting the selection is a picture of
    /// one item under the name of another.
    #[test]
    fn an_unresolvable_selection_previews_nothing() {
        assert_eq!(catalogue_only(true, Some("not-a-real-entry")), None);
    }

    /// #1288. `showing` is deliberately narrower than `staged`: a subject
    /// is on the stage the frame it is picked, but its meshes have no
    /// bounds yet, so the camera is still aimed where the PREVIOUS subject
    /// was. Drawing the target in that window shows the new item from the
    /// old item's distance. The panel asks `showing`, which stays silent
    /// until the framing has latched.
    #[test]
    fn a_staged_subject_is_not_shown_until_it_is_framed() {
        let subject = PreviewSubject::Catalogue(a_real_slug().to_string());
        let unframed = preview_showing(Some(subject.clone()), false);
        assert_eq!(unframed.staged(), Some(&subject));
        assert_eq!(
            unframed.showing(),
            None,
            "staged but not framed is not a picture"
        );

        let framed = preview_showing(Some(subject.clone()), true);
        assert_eq!(framed.showing(), Some(&subject));

        let empty = preview_showing(None, true);
        assert_eq!(empty.showing(), None, "framed with nothing on the stage");
    }

    /// #1288. The framing has to put the whole bounding sphere inside the
    /// vertical field of view with the margin to spare, at every scale the
    /// catalogue holds -- a 40 mm trinket and a 12 m building go through
    /// the same function.
    #[test]
    fn the_framing_fits_the_subject_at_every_scale() {
        for radius in [0.02f32, 0.5, 3.0, 12.0] {
            let centre = Vec3::new(1.0, 2_000.0, -3.0);
            let placement = camera_placement(centre, radius);
            let distance = placement.translation.distance(centre);

            // The half-angle the sphere subtends from where the camera
            // stands must be inside the camera's own half-angle.
            let subtended = (radius / distance).asin();
            assert!(
                subtended < VIEW_FOV * 0.5,
                "radius {radius} subtends {subtended} at {distance} m, \
                 which does not fit a {VIEW_FOV} fov"
            );
            // ...and not so far back that the item is a speck: the margin
            // is 25%, so the sphere must still fill most of the frame.
            assert!(
                subtended > VIEW_FOV * 0.5 / (FRAMING_MARGIN + 0.5),
                "radius {radius} is framed too small"
            );
            assert!(
                distance < 200.0,
                "radius {radius} is framed from {distance} m, outside the \
                 preview camera's far plane"
            );
        }
    }

    /// #1288. A subject with no geometry at all still has to leave the
    /// camera somewhere legal -- the near plane is 10 mm, and a zero
    /// radius through the fit arithmetic alone would put the camera on top
    /// of the stage.
    #[test]
    fn a_subject_with_no_size_still_gets_a_legal_camera() {
        let centre = Vec3::ZERO;
        let distance = camera_placement(centre, 0.0).translation.distance(centre);
        assert!(
            distance > 0.01,
            "the camera stands at {distance} m, inside its own near plane"
        );
    }

    /// #1288. The stage is parked far from the listener because a
    /// construct's audio is spatial, and near enough that `f32` world
    /// coordinates there still resolve well under a millimetre. It also
    /// has to stay OUT of the main camera's way only as a courtesy -- the
    /// render layer is the isolation -- so this pins the
    /// precision claim, which is the one that would fail silently.
    #[test]
    fn the_stage_sits_where_f32_still_resolves_sub_millimetre() {
        let ulp = STAGE_ORIGIN.y * f32::EPSILON;
        assert!(
            ulp < 0.001,
            "coordinates at {} m step in {ulp} m, which a preview would show",
            STAGE_ORIGIN.y
        );
    }
}

/// The propagation mechanism the whole isolation rests on, pinned on its
/// own (#1288).
///
/// `RenderLayers` does NOT inherit down the hierarchy in Bevy 0.19 -
/// only `Visibility` does - so the stage tags its root with
/// `Propagate<RenderLayers>` and relies on [`HierarchyPropagatePlugin`] to
/// reach every node the spawn path built, and every particle an emitter
/// adds afterwards. If a Bevy upgrade changes that, the preview silently
/// stops being isolated: its geometry appears in the world and the world
/// appears in its picture. This test costs no render and says so first.
#[cfg(test)]
mod propagation_tests {
    use super::*;
    use bevy::app::HierarchyPropagatePlugin;

    #[test]
    fn the_layer_reaches_a_child_the_spawn_path_never_tagged() {
        let mut app = App::new();
        app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate));

        let layers = RenderLayers::layer(PREVIEW_LAYER);
        let stage = app.world_mut().spawn(Propagate(layers.clone())).id();
        let node = app.world_mut().spawn(ChildOf(stage)).id();
        let deep = app.world_mut().spawn(ChildOf(node)).id();
        app.update();

        for (entity, what) in [(node, "a spawned node"), (deep, "a node under it")] {
            let got = app.world().entity(entity).get::<RenderLayers>();
            assert_eq!(
                got,
                Some(&layers),
                "{what} did not inherit the preview layer"
            );
        }

        // And a child added LATER - an emitter's particle - gets it too,
        // which is the case a one-shot tag at spawn time would have missed.
        let particle = app.world_mut().spawn(ChildOf(node)).id();
        app.update();
        assert_eq!(
            app.world().entity(particle).get::<RenderLayers>(),
            Some(&layers),
            "a child added after the stage was built did not inherit the layer"
        );

        // The world's own entities stay on layer 0, which is what the
        // preview camera must not be able to see.
        let outsider = app.world_mut().spawn_empty().id();
        app.update();
        assert!(
            app.world().entity(outsider).get::<RenderLayers>().is_none(),
            "propagation escaped the stage"
        );
    }
}

/// A person or a beast on the stage (#1842), through the real restage, the
/// real stand and the real framing, with the stage's builds held by the test
/// ([`held`]) and each real body built from the very job the stage asked for.
#[cfg(test)]
mod body_tests {
    use super::held::{HeldBuild, hold_builds, take_held};
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    use crate::catalogue::avatars::{avatar_by_slug, engine_body};
    use crate::offload::GenJob;
    use crate::pds::inventory::avatar_item_tests::{generator_record, rigged_record};
    use crate::pds::inventory::{AvatarItem, AvatarItemBody};

    const PERSON: &str = "avatar_person_young_woman";
    const BEAST: &str = "avatar_beast_horse";
    const VEHICLE: &str = "avatar_skiff_rover";

    /// The real spawn path's app, with transforms, the sibling crate's frame
    /// order and the layer propagation the stage relies on; builds held; the
    /// work counts zeroed.
    fn stage_app() -> App {
        let mut app = crate::player::visuals::spawn_path_app();
        app.add_plugins((
            bevy::transform::TransformPlugin,
            bevy_symbios_avatar::AvatarPlugin,
        ));
        crate::camera::register_render_layer_propagation(&mut app);
        app.init_resource::<PreviewRequest>();
        let camera = app
            .world_mut()
            .spawn((
                Camera {
                    is_active: false,
                    ..default()
                },
                PreviewCamera,
                Transform::default(),
            ))
            .id();
        app.world_mut().insert_resource(preview_for_test(camera));
        hold_builds();
        take_held();
        take_build_kicks();
        take_restage_count();
        app
    }

    /// One frame of the stage: the restage and the stand as `Update` runs
    /// them, `PostUpdate`'s propagation, then the framing.
    fn frame(app: &mut App) {
        let world = app.world_mut();
        world.run_system_once(restage_preview).expect("restages");
        world.run_system_once(stand_staged_body).expect("stands");
        app.update();
        app.world_mut()
            .run_system_once(frame_preview)
            .expect("frames");
    }

    fn ask(app: &mut App, subject: Option<PreviewSubject>) {
        app.world_mut().resource_mut::<PreviewRequest>().0 = subject;
    }

    fn preview(app: &App) -> &ItemPreview {
        app.world().resource::<ItemPreview>()
    }

    fn camera_on(app: &App) -> bool {
        let camera = preview(app).camera;
        app.world()
            .get::<Camera>(camera)
            .is_some_and(|camera| camera.is_active)
    }

    fn bodies(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query::<&BuiltBody>().iter(world).count()
    }

    /// The one build the stage has out, after checking it is the game's own
    /// job - the far hair tier, the draft atlas - for `record`.
    fn the_build(record: &EngineAvatarRecord) -> HeldBuild {
        let mut held = take_held();
        assert_eq!(held.len(), 1, "exactly one build kicked");
        let build = held.remove(0);
        match &build.job {
            GenJob::AvatarBuild {
                record: asked,
                atlas,
                far_hair,
                ..
            } => {
                assert_eq!(**asked, *record, "the subject's own engine record");
                assert_eq!(*atlas, draft_atlas(), "at the draft rung");
                assert!(*far_hair, "the game's own job, far tier and all");
            }
            _ => panic!("the stage asked for some other job"),
        }
        build
    }

    /// Frames until the picture is up, a handful at most: a landed body
    /// stands, then dresses, then waits a frame for its props.
    fn frame_until_shown(app: &mut App) {
        for _ in 0..6 {
            if preview(app).showing().is_some() {
                return;
            }
            frame(app);
        }
        panic!("the body never came up on the stage");
    }

    /// Everything under `entity`, itself included.
    fn tree_of(world: &World, entity: Entity) -> Vec<Entity> {
        let mut out = vec![entity];
        let mut at = 0;
        while at < out.len() {
            if let Some(children) = world.get::<Children>(out[at]) {
                out.extend(children.iter());
            }
            at += 1;
        }
        out
    }

    /// Every entity of the staged body - the stand-in chassis, the root, the
    /// joints the Bevy layer spawned, the meshes, the worn props - is on the
    /// stage's layer and nothing a world system finds a body or a part by.
    fn assert_isolated(app: &App) {
        let body = preview(app).shown_body().expect("a body on show");
        let world = app.world();
        let all = tree_of(world, body.stage);
        let joints = world
            .get::<AvatarJoints>(body.root)
            .expect("the engine's joints");
        for joint in &joints.0 {
            assert!(all.contains(joint), "a joint hangs off the stage");
        }
        let layer = RenderLayers::layer(PREVIEW_LAYER);
        for &entity in &all {
            let e = world.entity(entity);
            assert_eq!(
                e.get::<RenderLayers>(),
                Some(&layer),
                "{entity} is not on the stage's layer"
            );
            assert!(!e.contains::<avian3d::prelude::Collider>(), "a collider");
            assert!(!e.contains::<avian3d::prelude::RigidBody>(), "a body");
            assert!(!e.contains::<crate::state::LocalPlayer>(), "a player");
            assert!(!e.contains::<crate::state::RemotePeer>(), "a peer");
            assert!(!e.contains::<crate::player::RiggedRoot>(), "a rigged root");
            assert!(!e.contains::<bevy_symbios_avatar::Drive>(), "a drive");
            assert!(
                !e.contains::<bevy_symbios_avatar::AvatarDriver>(),
                "a driver"
            );
            assert!(
                !e.contains::<crate::player::gait::GaitAnimation>(),
                "a gait"
            );
            assert!(
                !e.contains::<crate::world_builder::RoomEntity>(),
                "a room tag"
            );
            assert!(
                !e.contains::<crate::world_builder::PrimMarker>(),
                "a gizmo key"
            );
            assert!(
                !e.contains::<crate::world_builder::AvatarVisualPrim>(),
                "an editor key"
            );
            assert!(
                !e.contains::<crate::world_builder::AttachmentPrim>(),
                "a worn part's editor key"
            );
            assert!(
                !e.contains::<crate::player::attachments::LocalAttachment>(),
                "a worn prop's editor identity"
            );
        }
    }

    /// The camera, as the box framing places it for the staged body alone -
    /// the posed body's own box, carried to the world by its root.
    fn framing_of_the_posed_body(app: &App) -> Transform {
        let body = preview(app).shown_body().expect("a body on show");
        let world = app.world();
        let built = world.get::<BuiltBody>(body.root).expect("built");
        let root = world.get::<GlobalTransform>(body.root).expect("placed");
        let (min, max) = standing_box(&built.avatar);
        let corners: Vec<Vec3> = box_corners(min, max)
            .map(|corner| root.transform_point(corner))
            .collect();
        body_placement(&corners)
    }

    /// A catalogue person is built by the game's own job, off the frame, and
    /// stands on the stage dressed, framed on its posed body - never on its
    /// skinned meshes' bind-pose bounds - and isolated as a tree is.
    #[test]
    fn a_catalogue_person_is_built_off_the_frame_and_stands_on_the_stage() {
        let mut app = stage_app();
        let entry = avatar_by_slug(PERSON).expect("an entry");
        let record = engine_body(entry.record().expect("builds"))
            .expect("rigged")
            .clone();
        let subject = PreviewSubject::CatalogueAvatar(PERSON.into());
        ask(&mut app, Some(subject.clone()));
        frame(&mut app);
        let build = the_build(&record);
        assert_eq!(preview(&app).staged(), Some(&subject), "staged at once");
        assert_eq!(preview(&app).showing(), None, "but nothing to show");
        assert!(!camera_on(&app), "no passes while the body builds");
        for _ in 0..10 {
            frame(&mut app);
        }
        assert_eq!(take_build_kicks(), 1, "ten idle frames kick nothing more");
        assert_eq!(bodies(&mut app), 0, "and stand nothing");

        build.build();
        frame(&mut app);
        assert_eq!(bodies(&mut app), 1, "the landed body stands");
        assert!(camera_on(&app), "and the camera goes on");
        // A bind-pose `Aabb` twenty metres across on every skinned mesh: if
        // the framing read them, the camera would stand far off.
        let world = app.world_mut();
        let skinned: Vec<Entity> = world
            .query_filtered::<Entity, With<SkinnedMesh>>()
            .iter(world)
            .collect();
        assert!(!skinned.is_empty());
        for entity in skinned {
            world.entity_mut(entity).insert(Aabb {
                center: Vec3::ZERO.into(),
                half_extents: Vec3::splat(10.0).into(),
            });
        }
        frame_until_shown(&mut app);
        assert_eq!(preview(&app).showing(), Some(&subject));
        let camera = app
            .world()
            .get::<Transform>(preview(&app).camera)
            .copied()
            .expect("a camera");
        let expected = framing_of_the_posed_body(&app);
        assert!(
            camera.translation.distance(expected.translation) < 1e-3,
            "framed at {:?}, the posed body's own framing is {:?}",
            camera.translation,
            expected.translation
        );
        assert_isolated(&app);
        assert_eq!(take_build_kicks(), 0, "and built once");
    }

    /// A stash person is built from the item, wears its prop under the
    /// joint the game's seating names - and on the stage's layer - and
    /// picking it again after something else builds nothing. A vehicle in
    /// between is staged as it always was: at once, from its tree, with no
    /// build.
    #[test]
    fn a_stash_person_wears_its_props_and_a_reselection_builds_nothing() {
        let mut app = stage_app();
        let mut stash = InventoryRecord::default();
        let person = AvatarItem::from_record(&rigged_record()).expect("copies");
        let AvatarItemBody::Rigged(rigged) = &person.body else {
            panic!("a rigged item");
        };
        let record = rigged.record.clone();
        stash.put_avatar("me".into(), person);
        stash.put_avatar(
            "ride".into(),
            AvatarItem::from_record(&generator_record()).expect("copies"),
        );
        app.world_mut().insert_resource(LiveInventoryRecord(stash));
        let me = PreviewSubject::Inventory {
            name: "me".into(),
            edit: Tick::new(1),
        };
        ask(&mut app, Some(me.clone()));
        frame(&mut app);
        the_build(&record).build();
        frame_until_shown(&mut app);
        assert_eq!(preview(&app).showing(), Some(&me));
        let body = preview(&app).shown_body().expect("on show");
        assert_eq!(body.props.len(), 1, "the circlet is worn");
        let joints = app
            .world()
            .get::<AvatarJoints>(body.root)
            .expect("joints")
            .0
            .clone();
        let carrier = app
            .world()
            .get::<ChildOf>(body.props[0])
            .expect("hung")
            .parent();
        assert!(joints.contains(&carrier), "from one of the body's joints");
        assert_isolated(&app);
        let stage = body.stage;
        assert_eq!(take_build_kicks(), 1, "the person was built once");

        // A vehicle: staged from its tree, at once, with no build.
        take_restage_count();
        let ride = PreviewSubject::Inventory {
            name: "ride".into(),
            edit: Tick::new(1),
        };
        ask(&mut app, Some(ride.clone()));
        frame(&mut app);
        assert_eq!(take_restage_count(), 1, "one restage");
        assert_eq!(take_build_kicks(), 0, "a vehicle builds nothing");
        assert!(preview(&app).stage.is_some(), "its tree is on the stage");
        assert!(camera_on(&app));
        assert_eq!(
            app.world().get::<Visibility>(stage),
            Some(&Visibility::Hidden),
            "the person is kept out of sight"
        );
        assert!(preview(&app).shown_body().is_none());

        // The person again: shown and dressed again, nothing built.
        ask(&mut app, Some(me.clone()));
        frame_until_shown(&mut app);
        assert_eq!(preview(&app).showing(), Some(&me));
        assert_eq!(take_build_kicks(), 0, "a re-selection builds nothing");
        assert_eq!(bodies(&mut app), 1, "the same body");
        assert_eq!(preview(&app).shown_body().map(|b| b.stage), Some(stage));
        assert_eq!(preview(&app).shown_body().map(|b| b.props.len()), Some(1));
        assert_isolated(&app);
        take_restage_count();
        for _ in 0..10 {
            frame(&mut app);
        }
        assert_eq!(
            take_restage_count(),
            0,
            "an idle selection restages nothing"
        );
        assert_eq!(take_build_kicks(), 0, "and builds nothing");
    }

    /// A body is framed with what it wears (#1845): the camera holds every
    /// corner of the worn prop's own bounds as well as the posed body's box,
    /// so it stands where the posed body's corners and the union of the
    /// props' bounds put it - not where the body alone would. The stage app
    /// computes no bounds, so here Bevy's own `calculate_bounds` runs as the
    /// renderer's frame runs it; without it no prop had an `Aabb`, and the
    /// union was never under test.
    #[test]
    fn a_bodys_worn_props_are_framed_with_it() {
        let mut app = stage_app();
        app.add_systems(PostUpdate, bevy::camera::visibility::calculate_bounds);
        let mut stash = InventoryRecord::default();
        let person = AvatarItem::from_record(&rigged_record()).expect("copies");
        let AvatarItemBody::Rigged(rigged) = &person.body else {
            panic!("a rigged item");
        };
        let record = rigged.record.clone();
        stash.put_avatar("me".into(), person);
        app.world_mut().insert_resource(LiveInventoryRecord(stash));
        ask(
            &mut app,
            Some(PreviewSubject::Inventory {
                name: "me".into(),
                edit: Tick::new(1),
            }),
        );
        frame(&mut app);
        the_build(&record).build();
        frame_until_shown(&mut app);

        let body = preview(&app).shown_body().expect("on show");
        assert_eq!(body.props.len(), 1, "the circlet is worn");
        let world = app.world();
        let layer = RenderLayers::layer(PREVIEW_LAYER);
        let boxes: Vec<(Vec3, Vec3)> = tree_of(world, body.props[0])
            .into_iter()
            .filter_map(|entity| {
                let e = world.entity(entity);
                let aabb = e.get::<Aabb>()?;
                assert_eq!(
                    e.get::<RenderLayers>(),
                    Some(&layer),
                    "on the stage's layer"
                );
                let centre = Vec3::from(aabb.center);
                let half = Vec3::from(aabb.half_extents);
                Some(transformed_box(
                    e.get::<GlobalTransform>()?,
                    (centre - half, centre + half),
                ))
            })
            .collect();
        assert!(!boxes.is_empty(), "the worn prop has bounds to frame by");
        let (props_min, props_max) = boxes.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), &(lo, hi)| (min.min(lo), max.max(hi)),
        );
        let prop_corners: Vec<Vec3> = boxes
            .iter()
            .flat_map(|&(lo, hi)| box_corners(lo, hi))
            .collect();

        let built = world.get::<BuiltBody>(body.root).expect("built");
        let root = world.get::<GlobalTransform>(body.root).expect("placed");
        let (min, max) = standing_box(&built.avatar);
        let body_corners: Vec<Vec3> = box_corners(min, max)
            .map(|corner| root.transform_point(corner))
            .collect();
        let held = |placement: &Transform, corner: Vec3| {
            let local = placement
                .compute_affine()
                .inverse()
                .transform_point3(corner);
            let depth = -local.z;
            let reach = (VIEW_FOV * 0.5).tan() / FRAMING_MARGIN + 1e-4;
            depth > 0.01 && local.x.abs() / depth <= reach && local.y.abs() / depth <= reach
        };
        // The premise: the prop reaches past what the body alone frames.
        let alone = framing_of_the_posed_body(&app);
        assert!(
            prop_corners.iter().any(|&corner| !held(&alone, corner)),
            "the prop sits inside the body's own framing, so this measures nothing"
        );

        let camera = world
            .get::<Transform>(preview(&app).camera)
            .copied()
            .expect("a camera");
        for &corner in body_corners.iter().chain(&prop_corners) {
            assert!(held(&camera, corner), "{corner} falls outside the margin");
        }
        let mut points = body_corners.clone();
        points.extend(box_corners(props_min, props_max));
        let expected = body_placement(&points);
        assert!(
            camera.translation.distance(expected.translation) < 1e-3,
            "framed at {:?}, the body and its props' framing is {:?}",
            camera.translation,
            expected.translation
        );
    }

    /// A newer selection while a build is out starts nothing until that build
    /// lands; then the newer one is built, and it is the newer body that
    /// stands - turned and framed as a beast.
    #[test]
    fn a_newer_selection_supersedes_a_build_in_flight() {
        let mut app = stage_app();
        let person = engine_body(
            avatar_by_slug(PERSON)
                .and_then(|e| e.record())
                .expect("builds"),
        )
        .expect("rigged")
        .clone();
        let beast = engine_body(
            avatar_by_slug(BEAST)
                .and_then(|e| e.record())
                .expect("builds"),
        )
        .expect("rigged")
        .clone();
        ask(
            &mut app,
            Some(PreviewSubject::CatalogueAvatar(PERSON.into())),
        );
        frame(&mut app);
        let first = the_build(&person);
        let horse = PreviewSubject::CatalogueAvatar(BEAST.into());
        ask(&mut app, Some(horse.clone()));
        for _ in 0..5 {
            frame(&mut app);
        }
        assert!(take_held().is_empty(), "no second build while one is out");

        // The stale build lands - here as the engine's failure, which must
        // not be read as the newer subject's.
        first.finish(GenResult::Avatar(None));
        frame(&mut app);
        assert_eq!(
            preview(&app).failed(),
            None,
            "the person's failure is not the horse's"
        );
        the_build(&beast).build();
        frame_until_shown(&mut app);
        assert_eq!(preview(&app).showing(), Some(&horse));
        assert_eq!(bodies(&mut app), 1, "only the newer body ever stood");
        let body = preview(&app).shown_body().expect("on show");
        let built = app.world().get::<BuiltBody>(body.root).expect("built");
        assert_eq!(
            built.avatar.rig.ground_contacts().len(),
            4,
            "the horse stands, on four legs"
        );
        assert_eq!(
            app.world().get::<Transform>(body.stage).map(|t| t.rotation),
            Some(body_facing(true)),
            "turned as a beast"
        );
    }

    /// A body that cannot be built stages nothing, keeps the camera off, and
    /// says so to its own pane only; picking it again builds nothing more.
    #[test]
    fn a_body_that_cannot_be_built_says_so() {
        use crate::ui::item_picture::{BODY_NOT_BUILT, PictureOf, failure_note};
        let mut app = stage_app();
        let person = engine_body(
            avatar_by_slug(PERSON)
                .and_then(|e| e.record())
                .expect("builds"),
        )
        .expect("rigged")
        .clone();
        let subject = PreviewSubject::CatalogueAvatar(PERSON.into());
        ask(&mut app, Some(subject.clone()));
        frame(&mut app);
        the_build(&person).finish(GenResult::Avatar(None));
        frame(&mut app);
        assert_eq!(preview(&app).failed(), Some(&subject));
        assert_eq!(preview(&app).showing(), None);
        assert!(!camera_on(&app));
        assert_eq!(bodies(&mut app), 0);
        assert_eq!(
            failure_note(Some(preview(&app)), PictureOf::CatalogueAvatar(PERSON)),
            Some(BODY_NOT_BUILT)
        );
        assert_eq!(
            failure_note(Some(preview(&app)), PictureOf::CatalogueAvatar(BEAST)),
            None,
            "another entry's pane says nothing"
        );
        assert_eq!(
            failure_note(Some(preview(&app)), PictureOf::Inventory(PERSON)),
            None
        );

        ask(
            &mut app,
            Some(PreviewSubject::CatalogueAvatar(VEHICLE.into())),
        );
        frame(&mut app);
        assert_eq!(preview(&app).failed(), None, "a vehicle has its picture");
        ask(&mut app, Some(subject.clone()));
        frame(&mut app);
        assert_eq!(
            take_build_kicks(),
            1,
            "the doomed body was built once, ever"
        );
        assert_eq!(preview(&app).failed(), Some(&subject));
    }

    /// **A winged body is turned as its posture goes** (#1875): a bird, on
    /// two legs, as a person; a bat or a glider, on four, as a beast. Persons
    /// and beasts as they were.
    #[test]
    fn a_winged_body_is_turned_as_its_posture_goes() {
        use symbios_avatar::{Archetype, QuadrupedParams, WingedFamily, WingedParams};
        let body = |archetype| EngineAvatarRecord {
            archetype,
            ..EngineAvatarRecord::default()
        };
        let winged = |family| {
            let mut params = WingedParams::default();
            params.set_kind(family, None, 0.0);
            body(Archetype::Winged(params))
        };
        assert!(!shows_its_flank(&body(Archetype::Humanoid(
            Default::default()
        ))));
        assert!(shows_its_flank(&body(Archetype::Quadruped(
            QuadrupedParams::default()
        ))));
        assert!(!shows_its_flank(&winged(WingedFamily::Bird)));
        assert!(shows_its_flank(&winged(WingedFamily::Bat)));
        assert!(shows_its_flank(&winged(WingedFamily::Glider)));
    }

    /// A person is turned to show the face in three quarters and a beast
    /// further round, its flank: the angle between where the body faces and
    /// where the camera stands, under the game's own root transform.
    #[test]
    fn a_person_and_a_beast_are_turned_to_the_camera() {
        for (beast, degrees) in [(false, PERSON_VIEW_DEG), (true, BEAST_VIEW_DEG)] {
            let chassis = Transform::from_rotation(body_facing(beast));
            let root = chassis.mul_transform(rigged_root_transform(0.9));
            // The engine's forward is +Z.
            let facing = root.rotation * Vec3::Z;
            let camera = Vec3::new(VIEW_YAW.sin(), 0.0, VIEW_YAW.cos());
            let seen = facing.dot(camera).clamp(-1.0, 1.0).acos().to_degrees();
            assert!(
                (seen - degrees).abs() < 0.01,
                "beast {beast}: the camera stands {seen} degrees off the face, not {degrees}"
            );
        }
    }

    /// The box framing holds every corner of a tall person's box and a long
    /// beast's - turned as the stage turns them - inside the square view with
    /// the margin to spare, and stands nearer than the sphere framing would,
    /// and nearer than a fit of the world box around the turned corners: the
    /// two reasons it exists.
    #[test]
    fn a_body_box_is_framed_whole_and_closer_than_its_sphere() {
        let centre = STAGE_ORIGIN + Vec3::Y;
        for (half, beast) in [
            (Vec3::new(0.3, 0.9, 0.15), false),
            (Vec3::new(0.35, 0.8, 1.2), true),
        ] {
            let turn = Transform::from_translation(centre)
                .with_rotation(body_facing(beast))
                .compute_affine();
            let corners: Vec<Vec3> = box_corners(-half, half)
                .map(|corner| turn.transform_point3(corner))
                .collect();
            let placement = body_placement(&corners);
            let reach = (VIEW_FOV * 0.5).tan();
            for &corner in &corners {
                let local = placement
                    .compute_affine()
                    .inverse()
                    .transform_point3(corner);
                let depth = -local.z;
                assert!(depth > 0.01, "a corner behind the near plane");
                assert!(
                    local.x.abs() / depth <= reach / FRAMING_MARGIN + 1e-4
                        && local.y.abs() / depth <= reach / FRAMING_MARGIN + 1e-4,
                    "box {half}: corner {corner} falls outside the margin"
                );
            }
            let sphere = camera_placement(centre, half.length());
            assert!(
                placement.translation.distance(centre) < sphere.translation.distance(centre),
                "box {half}: no nearer than its sphere"
            );
            let (min, max) = corners.iter().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(min, max), &p| (min.min(p), max.max(p)),
            );
            let world_box: Vec<Vec3> = box_corners(min, max).collect();
            assert!(
                placement.translation.distance(centre)
                    < body_placement(&world_box).translation.distance(centre),
                "box {half}: no nearer than the world box around it"
            );
        }
    }
}
