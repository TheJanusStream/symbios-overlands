//! In-world identity: overhead peer nametags, and the two-way link between
//! a People row and the body it names (#1226 f325).
//!
//! Every social action the product ships — Mute, Visit, drag-to-gift, the
//! mutual-★ — is keyed to a roster row, and until this module existed
//! nothing connected a row to the body standing in front of you. There was
//! no nametag, no overhead label, no hover identification and no picking
//! path for a peer's mesh, so "gift the lamp to @alice" and "mute whoever
//! just parked their avatar in my camera" were both guesswork — and every
//! wrong guess at the second one writes a durable, account-scoped block.
//!
//! Two halves, one fact:
//!
//! * [`measure_peer_nametags`] + [`peer_nametags_ui`] hang a name over each
//!   remote body. The text is [`PeerLabel`] and nothing else — the same
//!   ladder (handle → DID head → "A traveler") the roster row, the chat
//!   author tag, the arrival lines and the gift modal all print, so the
//!   name over the body is character-for-character the name in the list.
//! * [`PeerFocus`] is the link. Hovering a roster row lights that body with
//!   a wire box ([`draw_focused_peer_highlight`]) and brightens its tag;
//!   hovering a tag lights that row. Roster and world become two views of
//!   one list rather than two lists.
//!
//! **Visibility is not ours.** `network::lifecycle::sync_mute_visibility`
//! is the single writer of a peer chassis's [`Visibility`], and it already
//! hides a peer who is muted OR whose first transform sample has not played
//! out. This module *reads* that decision, so a muted body carries no name
//! and a peer still parked at the spawn pose carries no name, for free and
//! without a second opinion.
//!
//! **Nothing here takes pointer input.** The tags are painted straight onto
//! a background [`egui::Painter`] and hover is resolved by hit-testing the
//! pointer against the tag rects by hand. An interactive `egui::Area` over
//! the 3D scene would set `wants_pointer_input`, and
//! `camera::gate_camera_on_gui` reads exactly that — a hoverable label
//! floating in the middle of the viewport would silently break orbiting the
//! camera through it.

use bevy::camera::primitives::Aabb;
use bevy::ecs::hierarchy::Children;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::config::ui::nametag as cfg;
use crate::network::presence::{PeerLabel, PeerResolve, PeerStatus, peer_status};
use crate::state::{LocalSettings, RemotePeer, SocialResonance};

// ---------------------------------------------------------------------------
// The link between the roster and the world
// ---------------------------------------------------------------------------

/// Which peer the pointer is pointing at, on each of the two surfaces that
/// can name one (#1226 f325).
///
/// Rebuilt from scratch every frame by the surface that owns each field —
/// `ui::people::people_ui` owns [`Self::row`], [`peer_nametags_ui`] owns
/// [`Self::tag`] — because a hover is a fact about *this* frame and a stale
/// one aims a highlight at a body the user has already moved away from.
///
/// Written through [`Self::set_row`] / [`Self::set_tag`] rather than
/// directly: both surfaces run every frame, and an unconditional write
/// through the `ResMut` would mark the resource changed on all of them
/// (#879). Nothing filters on `Changed<PeerFocus>` today; the guard is
/// there so nothing has to check before it does.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct PeerFocus {
    /// The peer whose roster row is under the pointer.
    pub row: Option<Entity>,
    /// The peer whose overhead nametag is under the pointer.
    pub tag: Option<Entity>,
}

impl PeerFocus {
    /// Set the roster-row half, writing only on a change.
    ///
    /// Generic over the smart pointer so a system's `ResMut` and a test's
    /// `Mut` both fit: the guard is the point, and it must be impossible to
    /// exercise the guarded write in a test through a different door than
    /// the one the system uses.
    pub fn set_row(this: &mut impl std::ops::DerefMut<Target = Self>, peer: Option<Entity>) {
        if this.row != peer {
            this.row = peer;
        }
    }

    /// Set the nametag half, writing only on a change.
    pub fn set_tag(this: &mut impl std::ops::DerefMut<Target = Self>, peer: Option<Entity>) {
        if this.tag != peer {
            this.tag = peer;
        }
    }
}

// ---------------------------------------------------------------------------
// The pure half
// ---------------------------------------------------------------------------

/// Whether a peer's tag is drawn this frame and at what opacity, or `None`
/// for no tag at all.
///
/// Pure, and the whole gate: the setting, the chassis's own visibility, and
/// distance. Extracted because the system around it needs a camera, a
/// window and a live egui context to run at all — the same reason every
/// user-facing decision in `network::presence` is a free function.
///
/// `chassis_hidden` is `sync_mute_visibility`'s answer, not a re-derivation
/// of it: a muted peer and a peer who has never had a transform sample are
/// both `Visibility::Hidden`, and a name floating over an empty patch of
/// ground would be worse than no name at all.
pub fn nametag_alpha(enabled: bool, chassis_hidden: bool, distance_m: f32) -> Option<f32> {
    if !enabled || chassis_hidden || !distance_m.is_finite() || distance_m < 0.0 {
        return None;
    }
    if distance_m >= cfg::MAX_DISTANCE_M {
        return None;
    }
    if distance_m <= cfg::FADE_START_M {
        return Some(1.0);
    }
    // Linear across the fade band. A hard cutoff blinks a name off mid-stride
    // in exactly the frame the user is watching somebody walk away.
    let span = cfg::MAX_DISTANCE_M - cfg::FADE_START_M;
    let alpha = 1.0 - (distance_m - cfg::FADE_START_M) / span;
    (alpha >= cfg::MIN_ALPHA).then_some(alpha)
}

/// The text over a body: exactly what the roster row renders for the same
/// peer, mutual-★ included.
///
/// Deliberately [`PeerLabel::addressed`] and not a fifth spelling of the
/// ladder. The whole point of an overhead name is that the user can carry
/// it to the People window and find the row — a body labelled
/// `did:plc:z72i7hdy…` beside a row labelled `identifying…` is two names
/// for one stranger and helps nobody.
pub fn nametag_text(label: &PeerLabel, mutual: bool) -> String {
    if mutual {
        format!("★ {}", label.addressed())
    } else {
        label.addressed()
    }
}

/// Where a peer's tag hangs, in world space.
///
/// The top of the body's merged render bounds plus a clearance, so the tag
/// sits above a hat rather than through it, and so it works for every
/// chassis family the product ships — a humanoid, a skiff and an airship
/// are wildly different heights and none of them knows its own.
///
/// `bounds` is `None` for a peer with no rendered meshes yet: a body still
/// resolving from the PDS, or one whose visuals were cleared for a rebuild.
/// Those peers get the fallback height rather than no tag, because the tag
/// is the only thing on screen telling the user somebody is standing there.
pub fn nametag_anchor(origin: Vec3, bounds: Option<(Vec3, Vec3)>) -> Vec3 {
    match bounds {
        Some((min, max)) => Vec3::new(
            (min.x + max.x) * 0.5,
            max.y + cfg::HEAD_CLEARANCE_M,
            (min.z + max.z) * 0.5,
        ),
        None => origin + Vec3::Y * cfg::FALLBACK_HEIGHT_M,
    }
}

// ---------------------------------------------------------------------------
// Measurement
// ---------------------------------------------------------------------------

/// One peer's tag, resolved to screen space and ready to paint.
#[derive(Clone, Debug)]
pub struct PeerNametag {
    /// The peer chassis, so a hover can be reported back as [`PeerFocus`].
    pub peer: Entity,
    /// The name, already through [`nametag_text`].
    pub text: String,
    /// Whether the local user and this peer follow each other — the tag is
    /// drawn in the identity accent when they do, matching the roster's
    /// warm treatment of the same row.
    pub mutual: bool,
    /// The one loading/failure chip [`peer_status`] picked, if any.
    pub status: Option<PeerStatus>,
    /// Logical-viewport position of the anchor, in egui points.
    pub screen: egui::Pos2,
    /// Fade from [`nametag_alpha`].
    pub alpha: f32,
    /// Metres from the camera — painted far-to-near so a near tag lands on
    /// top of a far one rather than under it.
    pub distance: f32,
}

/// This frame's tags, in paint order (far first).
#[derive(Resource, Default)]
pub struct PeerNametags {
    pub tags: Vec<PeerNametag>,
}

/// Project every visible peer's head to screen space.
///
/// Runs in `PostUpdate` after `TransformSystems::Propagate` and before
/// bevy_egui's `EguiPostUpdateSet::EndPass`, which is where
/// `EguiPrimaryContextPass` — and therefore [`peer_nametags_ui`] — actually
/// runs. That ordering is the reason this is two systems and not one: the
/// egui pass has no declared order against transform propagation, so a
/// projection done inside it reads a `GlobalTransform` that may be a frame
/// stale, and a nametag that trails the body it names by one frame is
/// visibly wrong on anybody who is walking.
#[allow(clippy::type_complexity)]
pub fn measure_peer_nametags(
    mut tags: ResMut<PeerNametags>,
    settings: Res<LocalSettings>,
    cameras: Query<(&Camera, &GlobalTransform), crate::camera::IsWorldCamera>,
    peers: Query<(
        Entity,
        &RemotePeer,
        &PeerResolve,
        &GlobalTransform,
        &Visibility,
        Option<&SocialResonance>,
    )>,
    children: Query<&Children>,
    bounds_query: Query<(&Aabb, &GlobalTransform)>,
) {
    // Guarded-dirty (#879): the resource is rewritten every frame by design,
    // but an empty room must not keep marking it changed.
    if !tags.tags.is_empty() {
        tags.tags.clear();
    }
    if !settings.show_peer_nametags {
        return;
    }
    let Some((camera, camera_gt)) = cameras.iter().find(|(camera, _)| camera.is_active) else {
        return;
    };
    let eye = camera_gt.translation();

    for (entity, peer, resolve, gt, visibility, resonance) in peers.iter() {
        // `sync_mute_visibility` is the one writer; this reads its verdict.
        // `Inherited` on a root chassis means visible — peers are spawned
        // unparented, so there is no ancestor to inherit anything from.
        let hidden = *visibility == Visibility::Hidden;
        let anchor = nametag_anchor(
            gt.translation(),
            crate::editor_gizmo::subtree_world_bounds(entity, None, &children, &bounds_query),
        );
        let distance = eye.distance(anchor);
        let Some(alpha) = nametag_alpha(settings.show_peer_nametags, hidden, distance) else {
            continue;
        };
        let Ok(screen) = camera.world_to_viewport(camera_gt, anchor) else {
            // Behind the camera, past a clip plane, or the camera has no
            // viewport yet (headless). Not a failure worth reporting: the
            // body is not on screen either.
            continue;
        };
        let label = PeerLabel::new(peer.handle.as_deref(), peer.did.as_deref());
        let mutual = matches!(resonance, Some(SocialResonance::Mutual));
        tags.tags.push(PeerNametag {
            peer: entity,
            text: nametag_text(&label, mutual),
            mutual,
            // The same one-chip ladder the roster row shows, for the same
            // reason it exists there: "… arriving" is why the body in front
            // of you is a translucent stand-in, and the user is looking at
            // the body, not the list.
            status: peer_status(peer.did.is_some(), resolve),
            screen: egui::pos2(screen.x, screen.y),
            alpha,
            distance,
        });
    }
    tags.tags.sort_by(|a, b| b.distance.total_cmp(&a.distance));
}

// ---------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------

/// Paint this frame's tags, and report which one the pointer is over.
///
/// On [`egui::Order::Background`] so every floating window sits above the
/// tags: the People window in particular is often exactly where the user
/// has parked it over a crowd, and a name painted on top of the roster it
/// duplicates is noise.
pub fn peer_nametags_ui(
    mut contexts: EguiContexts,
    tags: Res<PeerNametags>,
    free: Res<crate::ui::layout::PanelFreeRect>,
    mut focus: ResMut<PeerFocus>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if tags.tags.is_empty() {
        PeerFocus::set_tag(&mut focus, None);
        return;
    }
    let theme = crate::ui::theme::current(ctx);
    let mut painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("peer-nametags"),
    ));
    // Clipped to what the toolbar left: a tag sliding under the top strip
    // must be cut off by it, not painted over it. egui 0.35 panels carve
    // the `Ui` they are shown into rather than the `Context`, so the
    // toolbar publishes its leftovers as `PanelFreeRect` (#1208) and
    // `ctx.available_rect()` no longer exists.
    let clip = free.0.unwrap_or_else(|| ctx.content_rect());
    painter.set_clip_rect(clip);

    // A pointer over a floating window is not over the world, whatever the
    // arithmetic says — the window is painted on top of the tag. Asked as
    // "is the topmost layer here the background?" because our own tags are
    // a bare painter and register no area: they can never answer yes to
    // this and shadow themselves.
    let pointer = ctx.pointer_latest_pos().filter(|&pos| {
        clip.contains(pos)
            && ctx
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order == egui::Order::Background)
    });
    let mut hovered = None;

    for tag in &tags.tags {
        let a = |c: egui::Color32| c.gamma_multiply(tag.alpha);
        let focused = focus.row == Some(tag.peer);
        let name_color = if tag.mutual {
            theme.accent
        } else {
            theme.text_strong
        };
        let name = painter.layout_no_wrap(
            tag.text.clone(),
            egui::FontId::proportional(13.0),
            a(name_color),
        );
        let status = tag.status.map(|status| {
            let color = if status.is_warning() {
                theme.status.warn
            } else {
                theme.text_weak
            };
            painter.layout_no_wrap(status.chip(), egui::FontId::proportional(10.0), a(color))
        });

        let width = name
            .rect
            .width()
            .max(status.as_ref().map_or(0.0, |g| g.rect.width()));
        let height = name.rect.height() + status.as_ref().map_or(0.0, |g| g.rect.height());
        // The anchor is the point in the world; the tag hangs above it.
        let rect = egui::Rect::from_min_size(
            egui::pos2(tag.screen.x - width * 0.5, tag.screen.y - height),
            egui::vec2(width, height),
        )
        .expand2(egui::vec2(5.0, 3.0));
        if !clip.intersects(rect) {
            continue;
        }

        // A plate behind the text, because the backdrop is a rendered world
        // and a bare label lands on grass, sky and stone in the same second.
        painter.rect_filled(rect, 4.0, a(theme.window_fill.gamma_multiply(0.75)));
        if focused {
            // The roster half of the link (#1226): the row under the
            // pointer says which body it is, in the accent the wire box
            // around that body is drawn in.
            painter.rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.5, a(theme.accent)),
                egui::StrokeKind::Outside,
            );
        }
        painter.galley(
            egui::pos2(tag.screen.x - name.rect.width() * 0.5, rect.top() + 3.0),
            name,
            egui::Color32::PLACEHOLDER,
        );
        if let Some(status) = status {
            painter.galley(
                egui::pos2(
                    tag.screen.x - status.rect.width() * 0.5,
                    rect.bottom() - 3.0 - status.rect.height(),
                ),
                status,
                egui::Color32::PLACEHOLDER,
            );
        }

        // Painted far-to-near, so the LAST tag containing the pointer is the
        // nearest one — which is the body the user means.
        if pointer.is_some_and(|p| rect.contains(p)) {
            hovered = Some(tag.peer);
        }
    }

    PeerFocus::set_tag(&mut focus, hovered);
}

/// Draw a wire box around the body whose roster row is under the pointer
/// (#1226 f325, the second half).
///
/// The same retained-free [`Gizmos`] path and the same subtree-bounds walk
/// the editor's selection highlight uses (#822), in the identity accent
/// rather than the editor's amber: hovering a *person* is not selecting an
/// object, and both boxes can be on screen at once.
///
/// Deliberately driven by [`PeerFocus::row`] alone. Lighting a body up
/// because the pointer drifted across its tag while orbiting the camera
/// would fire constantly and mean nothing; the roster hover is a
/// deliberate question — "which one is this?" — and deserves the answer.
pub fn draw_focused_peer_highlight(
    mut gizmos: Gizmos,
    focus: Res<PeerFocus>,
    peers: Query<Entity, With<RemotePeer>>,
    children: Query<&Children>,
    bounds_query: Query<(&Aabb, &GlobalTransform)>,
) {
    let Some(entity) = focus.row else {
        return;
    };
    // The row may name a peer who disconnected between the egui pass and
    // now; a box around a despawned entity would be a box at the origin.
    if peers.get(entity).is_err() {
        return;
    }
    let Some((min, max)) =
        crate::editor_gizmo::subtree_world_bounds(entity, None, &children, &bounds_query)
    else {
        return;
    };
    let color = Color::srgba(
        cfg::FOCUS_BOX_COLOR[0],
        cfg::FOCUS_BOX_COLOR[1],
        cfg::FOCUS_BOX_COLOR[2],
        cfg::FOCUS_BOX_COLOR[3],
    );
    gizmos.cube(
        Transform {
            translation: (min + max) * 0.5,
            rotation: Quat::IDENTITY,
            scale: (max - min).max(Vec3::splat(cfg::MIN_FOCUS_BOX_EXTENT)),
        },
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1226 f325. The sequence: somebody parks their avatar in your camera
    /// and you mute them — and the body must stop carrying a name in the
    /// same frame it stops being drawn. `sync_mute_visibility` owns that
    /// decision for both, which is the whole reason this reads a
    /// `Visibility` instead of re-deriving `peer.muted || !resolve.placed`.
    #[test]
    fn a_hidden_chassis_carries_no_name() {
        assert_eq!(nametag_alpha(true, true, 5.0), None);
        // ...and the same peer, once unmuted and placed, does.
        assert_eq!(nametag_alpha(true, false, 5.0), Some(1.0));
    }

    /// The setting is the whole opt-out: a room full of people is a wall of
    /// text for somebody who does not want one.
    #[test]
    fn the_setting_switches_every_tag_off() {
        assert_eq!(nametag_alpha(false, false, 1.0), None);
    }

    /// The sequence a hard cutoff breaks: you watch somebody walk away and
    /// their name blinks out of existence in one frame, which reads as a
    /// glitch rather than as distance.
    #[test]
    fn a_name_fades_with_distance_rather_than_blinking_off() {
        let near = nametag_alpha(true, false, cfg::FADE_START_M - 1.0).expect("inside the band");
        let mid = nametag_alpha(true, false, (cfg::FADE_START_M + cfg::MAX_DISTANCE_M) * 0.5)
            .expect("mid-fade");
        assert_eq!(near, 1.0);
        assert!(mid < near, "the tag dims across the fade band");
        assert!(mid >= cfg::MIN_ALPHA);
        assert_eq!(
            nametag_alpha(true, false, cfg::MAX_DISTANCE_M),
            None,
            "past the ceiling there is no tag at all"
        );
        assert!(
            nametag_alpha(true, false, cfg::MAX_DISTANCE_M - 0.01).is_none(),
            "the last slice of the band is below MIN_ALPHA, so it is dropped \
             rather than drawn invisibly"
        );
    }

    /// A NaN distance is reachable: a peer whose transform buffer produced a
    /// degenerate sample, or a camera at the same point as the anchor under
    /// a zero-scale parent. It must not paint a tag at an arbitrary place.
    #[test]
    fn a_degenerate_distance_draws_nothing() {
        assert_eq!(nametag_alpha(true, false, f32::NAN), None);
        assert_eq!(nametag_alpha(true, false, f32::INFINITY), None);
        assert_eq!(nametag_alpha(true, false, -1.0), None);
    }

    /// #1226 f325's core: the name over the body and the name in the list
    /// are the same string, on every rung of the ladder. Two spellings of
    /// one stranger is the defect, not a cosmetic difference.
    #[test]
    fn the_tag_prints_the_same_ladder_the_roster_row_does() {
        let handle = PeerLabel::new(Some("alice.bsky.social"), Some("did:plc:abcdefghijkl"));
        let did = PeerLabel::new(None, Some("did:plc:abcdefghijklmnop"));
        let anon = PeerLabel::new(None, None);
        assert_eq!(nametag_text(&handle, false), handle.addressed());
        assert_eq!(nametag_text(&did, false), did.addressed());
        assert_eq!(nametag_text(&anon, false), anon.addressed());
        // The mutual star is the roster's decoration, in the roster's order.
        assert_eq!(
            nametag_text(&handle, true),
            format!("★ {}", handle.addressed())
        );
        // The `@` sigil belongs to the Handle tier and nothing else (#1218
        // f299) — a tag is one more surface that must not invent one.
        assert!(!nametag_text(&did, true).contains('@'));
        assert!(!nametag_text(&anon, false).contains('@'));
    }

    /// The tag hangs above the body's *rendered* bounds, not above its
    /// origin: an airship and a humanoid are both peers, and a fixed offset
    /// puts one name in the hull and the other in the sky.
    #[test]
    fn the_anchor_clears_the_top_of_the_body_whatever_shape_it_is() {
        let tall = nametag_anchor(
            Vec3::ZERO,
            Some((Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 6.0, 1.0))),
        );
        let short = nametag_anchor(
            Vec3::ZERO,
            Some((Vec3::new(-0.3, 0.0, -0.3), Vec3::new(0.3, 1.7, 0.3))),
        );
        assert!(tall.y > short.y);
        assert!((tall.y - (6.0 + cfg::HEAD_CLEARANCE_M)).abs() < 1e-5);
        // Centred over the body's footprint, not over its transform origin.
        let offset = nametag_anchor(
            Vec3::new(10.0, 0.0, 0.0),
            Some((Vec3::new(10.0, 0.0, 4.0), Vec3::new(14.0, 2.0, 6.0))),
        );
        assert!((offset.x - 12.0).abs() < 1e-5);
        assert!((offset.z - 5.0).abs() < 1e-5);
    }

    /// A peer with no meshes yet — the body is still being fetched — still
    /// gets a name, because the name is the only thing on screen saying
    /// somebody is standing there.
    #[test]
    fn a_body_that_has_not_built_yet_still_gets_a_name_above_it() {
        let anchor = nametag_anchor(Vec3::new(3.0, 1.0, -2.0), None);
        assert_eq!(anchor.x, 3.0);
        assert_eq!(anchor.z, -2.0);
        assert!(
            anchor.y > 1.0,
            "the tag is above the chassis, not inside it"
        );
    }

    /// #879 applied to the link: both writers run every frame, and an
    /// unconditional write would mark the resource changed on all of them.
    #[test]
    fn an_unchanged_hover_does_not_dirty_the_link() {
        let mut world = World::new();
        world.init_resource::<PeerFocus>();
        let peer = world.spawn_empty().id();

        fn set_row(world: &mut World, value: Option<Entity>) -> bool {
            let mut focus = world.resource_mut::<PeerFocus>();
            PeerFocus::set_row(&mut focus, value);
            focus.is_changed()
        }
        assert!(set_row(&mut world, Some(peer)), "a new hover is a change");
        world.clear_trackers();
        assert!(
            !set_row(&mut world, Some(peer)),
            "the same hover, held, is not"
        );
        world.clear_trackers();
        assert!(set_row(&mut world, None), "letting go is a change");
    }

    /// The two halves are independent: the roster owns one, the world owns
    /// the other, and neither may clear the other's answer.
    #[test]
    fn each_surface_owns_only_its_own_half_of_the_link() {
        let mut world = World::new();
        world.init_resource::<PeerFocus>();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        {
            let mut focus = world.resource_mut::<PeerFocus>();
            PeerFocus::set_row(&mut focus, Some(a));
            PeerFocus::set_tag(&mut focus, Some(b));
        }
        {
            let mut focus = world.resource_mut::<PeerFocus>();
            PeerFocus::set_row(&mut focus, None);
        }
        let focus = world.resource::<PeerFocus>();
        assert_eq!(focus.row, None);
        assert_eq!(focus.tag, Some(b), "the world's half survived");
    }
}
