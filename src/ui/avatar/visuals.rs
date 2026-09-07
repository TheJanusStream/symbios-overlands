//! The Visuals tab: a **construction-kit** body's generator tree (#1161).
//!
//! There is no drawing code of its own here. The tab embeds the same
//! tree-view + detail-panel widget that drives the room editor's Generators
//! tab, fed by an [`AvatarVisualsTreeSource`] adapter so a generator body's
//! tree is editable through the unified vocabulary — one widget, three
//! hosts (this, the room, and a worn item's parts editor).
//!
//! What lives here is the wiring that is *this* host's: seeding the tree
//! panel's selection from the editor's single gizmo aim before the draw,
//! and folding the owner's click back into it afterwards. A rigged body has
//! no such tree, and the tab says so rather than drawing an empty one.

use bevy_egui::egui;

use super::{AimCtx, GizmoTarget, TabCtx};
use crate::ui::room::generators::{AvatarVisualsTreeSource, TreeSelection, draw_generators_tab};

/// The Visuals tab, wired to the editor (#1161).
pub(super) fn draw_tab(ui: &mut egui::Ui, ctx: &mut TabCtx, aim: &mut AimCtx, height: f32) {
    // Read before the record is borrowed mutably below — see the note on
    // `attachments::draw_tab`.
    let owner_did = ctx.owner_did();
    ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
        // The tree edits a generator body's tree; a rigged body has no tree
        // to draw. #1265 f101: this used to promise the rigged editor was
        // still coming (it shipped, as the Body tab) and to advise a bare
        // re-roll, which lands back on a rigged body whenever
        // `ChassisFamily::for_seed` rolls `Humanoid` — one of four families,
        // so a coin flip. The Chassis pin row is the deterministic control,
        // so the advice routes through it and names the three families by
        // the labels that row actually shows (`ChassisFamily::label`).
        let Some(visuals) = ctx.record.body.visuals_mut() else {
            ui.label(
                egui::RichText::new(
                    "You're wearing a rigged body — sculpt it on the \
                     Body tab. For a construction-kit body instead, \
                     open Seed & re-roll below, lock Chassis to \
                     Hover-boat, Airship or Land-skiff, and re-roll.",
                )
                .small()
                .weak(),
            );
            return;
        };
        let mut source = AvatarVisualsTreeSource::new(visuals);
        // The panel's selection is the widget's I/O, not the truth: seed it
        // from the aim, fold it back after (#1161). The one-shot focus
        // request rides the panel now, consumed by the tree it belongs to.
        let aimed = aim.gizmo.visuals_path().map(<[usize]>::to_vec);
        aim.visuals_tree.selection = TreeSelection {
            root: aimed
                .as_ref()
                .map(|_| AvatarVisualsTreeSource::ROOT_NAME.to_string()),
            path: aimed.clone(),
        };
        draw_generators_tab(
            ui,
            &mut source,
            aim.visuals_tree,
            ctx.inventory.as_deref_mut(),
            ctx.audio_editor,
            ctx.grammar_diag,
            ctx.changed,
            ctx.blob_selected_element,
            ctx.toasts,
            ctx.now,
            &mut ctx.labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
            // Avatars can't grow roads — no stats readout.
            None,
            ctx.face_pick,
            owner_did,
            &mut None,
            &mut String::new(),
            ctx.clipboard,
            ctx.assets,
        );
        if aim.visuals_tree.selection.path != aimed {
            aim.aim(match aim.visuals_tree.selection.path.clone() {
                Some(path) => GizmoTarget::VisualsNode { path },
                None => GizmoTarget::None,
            });
        }
    });
}
