//! Item preview — a live 3D picture of the selected catalogue entry
//! (#1288).
//!
//! The Catalogue lists 392 entries by name and description alone, so the
//! only way to find out what one looks like was to copy it into the stash
//! and wear or place it. This module renders the *selected* entry into an
//! off-screen texture that [`crate::ui::catalogue`]'s detail panel draws
//! as an ordinary `egui::Image`, which is why it is not
//! [`crate::avatar::draw_avatar_icon`]'s neighbour: the profile-picture
//! path uploads bytes fetched from a PDS, and this one owns a camera.
//!
//! The owner chose render-on-selection over a baked atlas: nothing here
//! goes stale when [`crate::catalogue::ENTRIES`] gains a row or #972
//! reworks one, and no image asset enters the repo or the wasm bundle.
//! The accepted cost is that the picture exists only for the SELECTED
//! entry — the browse tree and the Inventory rows stay text-only.
//!
//! ## The layer is the isolation; the distance is for something else
//!
//! The preview subject is spawned through the same avatar visual path a
//! worn prop uses, into a "stage" rooted at [`STAGE_ORIGIN`], and
//! everything on it carries [`PREVIEW_LAYER`]. The layer is what makes the
//! world camera blind to the stage and the preview camera blind to the
//! world: it is the isolation, and the distance is not.
//!
//! **`RenderLayers` does not propagate down the hierarchy in Bevy 0.19** —
//! only `Visibility` does — and the spawn path builds a whole tree, whose
//! particle emitters go on adding children later. So the stage root
//! carries `Propagate<RenderLayers>` and the app registers
//! [`bevy::app::HierarchyPropagatePlugin`] for it in
//! `PostUpdate`, ordered before `VisibilitySystems::CheckVisibility`, so
//! the component lands on descendants — including ones spawned this frame
//! — before anything decides what the world camera can see. Inserting the layer
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
//! sub-millimetre, which the far side of the main camera's 12 km far
//! plane would not be.
//!
//! ## What it costs, which is a wasm question first
//!
//! One 256 px `Rgba8UnormSrgb` target — a quarter of a megabyte of VRAM,
//! declared `RenderAssetUsages::RENDER_WORLD` so it keeps no main-world
//! CPU copy (#565 measured that retention as the dominant wasm cost, and
//! a wasm heap never shrinks). The camera is spawned ONCE and toggled by
//! [`restage_preview`], never spawned per selection, and it is inactive
//! whenever the Catalogue window is shut — so a session that never opens
//! the Catalogue pays the target's memory and no passes at all. `Msaa` is
//! off for the same reason the main camera turns it off: Bevy's default
//! `Sample4` panics on the WebGL2 entry point.
//!
//! It adds no sampler to any existing material, so the WebGL2 16-sampler
//! ceiling the splat material sits against is untouched — this is a
//! second view, not a second texture on the first one.
//!
//! ## What the spawn path does NOT bring
//!
//! The subject is spawned with `avatar_mode` on and `is_local` off, which
//! is load-bearing: that mode strips colliders, the `RoomEntity` /
//! `PlacementUnit` cleanup tags and the `PrimMarker` gizmo key. A preview
//! is therefore not a physics body, not something a room rebuild sweeps,
//! and not something the gizmo can find. The one marker it does carry is
//! `AvatarVisualRoot`, which only the gait layer reads — and that layer
//! looks roots up *from* an entity that has a `GaitAnimation`, so a stage
//! with no such parent is never reached.

use bevy::app::{HierarchyPropagatePlugin, Propagate};
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy_egui::{EguiTextureHandle, EguiUserTextures};

use crate::player::visuals::{AvatarSpawnDeps, spawn_visual_tree};
use crate::state::AppState;

/// The render layer the preview stage, its key light and its camera live
/// on. Nothing else in the crate uses `RenderLayers` at all, so layer 0 —
/// the layer every component-less entity belongs to — is the whole rest of
/// the app.
pub const PREVIEW_LAYER: usize = 1;

/// Where the stage sits. Two kilometres up: far enough that a construct's
/// spatial audio is inaudible from the listener, near enough that `f32`
/// world coordinates there still resolve well under a millimetre.
const STAGE_ORIGIN: Vec3 = Vec3::new(0.0, 2_000.0, 0.0);

/// Edge of the square render target, in pixels. The detail panel draws it
/// at a smaller logical size, so this is the resolution the picture is
/// sampled *from* — 256 keeps it crisp on a 2x display without costing a
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
/// Floor on how close the camera will stand, in metres — five times its
/// own near plane, so a subject with no size cannot put geometry inside
/// it. Deliberately only just above the near plane: a larger floor is not
/// a safety margin, it is a rule that renders every small item small, and
/// the catalogue's smallest entries are centimetres across.
const MIN_VIEW_DISTANCE: f32 = 0.05;

/// What the preview is showing, or is being asked to show. Compared by
/// value: a restage happens when — and only when — this differs from what
/// is already on the stage, so holding a selection costs one camera pass
/// per frame and no respawns.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PreviewSubject {
    /// A catalogue entry, by [`crate::catalogue::CatalogueEntry::slug`].
    Catalogue(String),
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
    /// bounds — a freshly spawned tree has no `Aabb` until Bevy has
    /// computed one, so framing is a follow-up, not part of the spawn.
    framed: bool,
}

impl ItemPreview {
    /// What the render target actually holds a usable picture OF.
    ///
    /// Deliberately narrower than "what is staged". A subject spawned this
    /// frame is staged immediately but not yet FRAMED — its meshes have no
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
}

pub struct ItemPreviewPlugin;

impl Plugin for ItemPreviewPlugin {
    fn build(&self, app: &mut App) {
        // See the module header: the stage's descendants — including
        // particles spawned after the tree — get their layer from here,
        // in PostUpdate, which is before visibility is computed and
        // before the render world extracts.
        app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate))
            // The plugin only says WHICH schedule; ordering it before
            // visibility is what makes the claim above true. Without this
            // the propagation could land after the frame's visibility
            // check and a freshly spawned preview node — or a particle the
            // emitter added this frame — would be drawn by the WORLD
            // camera once, two kilometres over the player's head.
            .configure_sets(
                PostUpdate,
                bevy::app::PropagateSet::<RenderLayers>::default()
                    .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
            )
            .add_systems(Startup, setup_preview)
            .add_systems(Update, restage_preview.run_if(in_state(AppState::InGame)))
            .add_systems(
                PostUpdate,
                frame_preview.after(bevy::transform::TransformSystems::Propagate),
            )
            // Leaving the game does not close the Catalogue window, so
            // without this the stage and its camera pass would survive a
            // logout into the login screen — where the resources the
            // restage needs are gone and it cannot clean up after itself.
            .add_systems(OnExit(AppState::InGame), clear_preview);
    }
}

/// Create the render target, hand it to egui, and spawn the camera and key
/// light that will look at whatever the stage holds. Runs once; the camera
/// starts inactive because nothing is selected yet.
fn setup_preview(
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
            Camera {
                // Ahead of the main camera, so the texture egui samples
                // this frame was drawn this frame.
                order: -1,
                clear_color: ClearColorConfig::Custom(BACKDROP),
                // Off until something is selected. A preview nobody is
                // looking at must not cost a render pass — this is the
                // whole reason the camera is spawned once and toggled
                // rather than spawned per selection.
                is_active: false,
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            // The web build's `WebGL2` path panics on Bevy's default
            // `Msaa::Sample4`, the same reason the main camera turns it
            // off (see `crate::camera`).
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
    // shadow cascades sized for a world. Shadows are off outright — a
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
    });
}

/// Take everything down: despawn the stage and switch the camera off.
/// Idempotent, so it is safe on a state exit that never had a preview.
fn clear_preview(
    mut commands: Commands,
    mut preview: ResMut<ItemPreview>,
    mut cameras: Query<&mut Camera>,
) {
    if let Some(stage) = preview.stage.take() {
        commands.entity(stage).despawn();
    }
    preview.staged = None;
    preview.framed = false;
    if let Ok(mut camera) = cameras.get_mut(preview.camera) {
        camera.is_active = false;
    }
}

/// The subject the UI is asking for: the Catalogue's selected entry, while
/// the Catalogue window is open. Derived rather than pushed — there is no
/// request resource for a panel to write every frame, so there is no
/// change-tick to guard (#879).
fn wanted_subject(catalogue_open: bool, selected: Option<&str>) -> Option<PreviewSubject> {
    // A closed window is the whole "is anyone looking" question: it is what
    // turns the camera off, and a camera pass nobody can see is the one
    // cost this approach could have carried and does not.
    if !catalogue_open {
        return None;
    }
    let slug = selected?;
    // A selection that no longer resolves shows nothing rather than the
    // last thing that did.
    crate::catalogue::by_slug(slug)?;
    Some(PreviewSubject::Catalogue(slug.to_string()))
}

/// Swap the stage over when the selection changes, and switch the camera
/// off when there is nothing selected.
#[allow(clippy::too_many_arguments)]
fn restage_preview(
    mut commands: Commands,
    mut preview: ResMut<ItemPreview>,
    panels: Res<crate::ui::toolbar::UiPanels>,
    browser: Res<crate::ui::catalogue::CatalogueBrowser>,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
    mut cameras: Query<&mut Camera>,
) {
    let wanted = wanted_subject(panels.catalogue, browser.selected_slug());
    if wanted.as_ref() == preview.staged() {
        return;
    }

    if let Some(stage) = preview.stage.take() {
        commands.entity(stage).despawn();
    }
    preview.framed = false;

    let Some(subject) = wanted else {
        preview.staged = None;
        if let Ok(mut camera) = cameras.get_mut(preview.camera) {
            camera.is_active = false;
        }
        return;
    };

    let PreviewSubject::Catalogue(slug) = &subject;
    let Some(entry) = crate::catalogue::by_slug(slug) else {
        preview.staged = None;
        return;
    };
    // The DID an entry builds against personalises a handful of items (a
    // gateway's plaque, a monument's picture). Signed out, an empty DID is
    // what the placement-check path already passes.
    let did = session.as_ref().map(|s| s.did.as_str()).unwrap_or("");
    let generator = entry.build(did);

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

/// Point the camera at the staged subject once its bounds exist.
///
/// A tree spawned this frame has no `Aabb` yet — Bevy computes one when
/// the mesh asset resolves — so the framing cannot be part of the spawn.
/// It retries every frame until it finds geometry, then latches: the
/// camera must not drift while a particle plume changes the bounds
/// underneath it.
fn frame_preview(
    mut preview: ResMut<ItemPreview>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
    bounds: Query<(&GlobalTransform, &Aabb, &RenderLayers)>,
) {
    if preview.framed || preview.stage.is_none() {
        return;
    }
    let layers = RenderLayers::layer(PREVIEW_LAYER);
    let Some((centre, radius)) = stage_bounds(&bounds, &layers) else {
        return;
    };
    let Ok(mut transform) = cameras.get_mut(preview.camera) else {
        return;
    };
    *transform = camera_placement(centre, radius);
    preview.framed = true;
}

/// Where the camera stands to see a bounding sphere of `radius` at
/// `centre`: the fixed three-quarter view, backed off far enough that the
/// sphere fits the vertical field of view with [`FRAMING_MARGIN`] to
/// spare.
///
/// [`MIN_VIEW_DISTANCE`] is what keeps a degenerate subject — a flat sign, a
/// single particle anchor with no geometry around it — from putting the
/// near plane inside itself.
fn camera_placement(centre: Vec3, radius: f32) -> Transform {
    let elevation = VIEW_ELEVATION_DEG.to_radians();
    let distance = (radius / (VIEW_FOV * 0.5).sin() * FRAMING_MARGIN).max(MIN_VIEW_DISTANCE);
    let horizontal = distance * elevation.cos();
    let offset = Vec3::new(
        horizontal * VIEW_YAW.sin(),
        distance * elevation.sin(),
        horizontal * VIEW_YAW.cos(),
    );
    Transform::from_translation(centre + offset).looking_at(centre, Vec3::Y)
}

/// Union of the world-space bounds of everything on the preview layer →
/// `(centre, bounding radius)`, or `None` while the stage has no geometry.
///
/// Reading the layer rather than walking the hierarchy is what keeps this
/// honest about particles: an emitter's children are separate entities
/// that the propagation has already reached, and a hierarchy walk from the
/// stage root would have to re-derive that relationship.
fn stage_bounds(
    bounds: &Query<(&GlobalTransform, &Aabb, &RenderLayers)>,
    layers: &RenderLayers,
) -> Option<(Vec3, f32)> {
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let mut any = false;
    for (transform, aabb, entity_layers) in bounds.iter() {
        if !entity_layers.intersects(layers) {
            continue;
        }
        any = true;
        let centre = Vec3::from(aabb.center);
        let half = Vec3::from(aabb.half_extents);
        for sx in [-1.0f32, 1.0] {
            for sy in [-1.0f32, 1.0] {
                for sz in [-1.0f32, 1.0] {
                    let corner = transform
                        .transform_point(centre + Vec3::new(sx * half.x, sy * half.y, sz * half.z));
                    min = min.min(corner);
                    max = max.max(corner);
                }
            }
        }
    }
    if !any {
        return None;
    }
    Some(((min + max) * 0.5, ((max - min) * 0.5).length().max(0.05)))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Any real slug, so the tests below say something about the actual
    /// catalogue rather than about a string literal.
    fn a_real_slug() -> &'static str {
        crate::catalogue::ENTRIES[0].slug()
    }

    fn preview_showing(subject: Option<PreviewSubject>, framed: bool) -> ItemPreview {
        ItemPreview {
            egui_texture: bevy_egui::egui::TextureId::User(0),
            camera: Entity::PLACEHOLDER,
            stage: None,
            staged: subject,
            framed,
        }
    }

    /// #1288. The gate that decides whether a render pass happens at all.
    /// A closed Catalogue is the "nobody is looking" answer, and it is the
    /// only thing standing between this feature and a camera that draws a
    /// 256 px texture every frame for the whole session.
    #[test]
    fn nothing_is_previewed_while_the_catalogue_is_closed() {
        assert_eq!(wanted_subject(false, Some(a_real_slug())), None);
        assert_eq!(
            wanted_subject(true, Some(a_real_slug())),
            Some(PreviewSubject::Catalogue(a_real_slug().to_string())),
            "an open window over a real selection is the one case that renders"
        );
        assert_eq!(wanted_subject(true, None), None, "nothing selected");
    }

    /// #1288. A slug the catalogue no longer answers to shows NOTHING,
    /// not whatever was last on the stage. The Catalogue's selection is a
    /// `String` held across frames while `ENTRIES` is a compile-time list,
    /// so the two can disagree the moment an entry is renamed or dropped
    /// -- and the failure mode of trusting the selection is a picture of
    /// one item under the name of another.
    #[test]
    fn an_unresolvable_selection_previews_nothing() {
        assert_eq!(wanted_subject(true, Some("not-a-real-entry")), None);
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
    /// has to stay OUT of the main camera's 12 km far plane's way only as
    /// a courtesy -- the render layer is the isolation -- so this pins the
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
/// `RenderLayers` does NOT inherit down the hierarchy in Bevy 0.19 —
/// only `Visibility` does — so the stage tags its root with
/// `Propagate<RenderLayers>` and relies on [`HierarchyPropagatePlugin`] to
/// reach every node the spawn path built, and every particle an emitter
/// adds afterwards. If a Bevy upgrade changes that, the preview silently
/// stops being isolated: its geometry appears in the world and the world
/// appears in its picture. This test costs no render and says so first.
#[cfg(test)]
mod propagation_tests {
    use super::*;

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

        // And a child added LATER — an emitter's particle — gets it too,
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
