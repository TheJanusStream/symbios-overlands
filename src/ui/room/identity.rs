//! The World Editor's Name and banner section (#1783): the world's own name
//! and its banner - two colours, a division of the field and a mark - with
//! a picture of it as it flies, and suggestions drawn as a seeded world
//! draws its own.
//!
//! The name is the owner's claim, shown beside their verified handle and
//! never in its place ([`crate::pds::identity`]): a draft the sanitiser
//! would change - one starting with a handle's `@`, a DID, one that reads
//! as a handle, one too long - is refused as it is typed, with the reason
//! ([`world_name_refusal`]), rather than changed under the cursor. What
//! paints as nothing and runs of spaces are dropped as it is applied.

use bevy_egui::egui;

use super::widgets::{color_picker, text_draft_row};
use crate::pds::identity::{clean_world_name, world_name_refusal};
use crate::pds::{BannerDivision, BannerMark, WorldIdentity};
use crate::seeded_defaults::SceneCharacter;

/// What a suggestion is drawn from: the world's seed - its land - and the
/// borough a Berlin square lies in.
#[derive(Clone, Copy)]
pub(super) struct Suggestions {
    pub seed: u64,
    pub berlin: Option<geodata::berlin::Borough>,
}

impl Suggestions {
    /// The `n`th suggestion for the world: the 0th is the one a seeded
    /// world derives, and each after it another of its land's.
    fn nth(self, n: u64) -> WorldIdentity {
        let scene = match self.berlin {
            Some(_) => SceneCharacter::even(self.seed),
            None => SceneCharacter::for_seed(self.seed),
        };
        let stream = self
            .seed
            .wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        crate::seeded_defaults::room::identity::derive(&scene, stream, self.berlin)
    }
}

/// The section's header: its name, and the world's.
pub(super) fn header(identity: Option<&WorldIdentity>) -> String {
    match identity.map(|identity| identity.name.as_str()) {
        Some(name) if !name.is_empty() => format!("Name and banner: {name}"),
        _ => "Name and banner".to_string(),
    }
}

/// Draw the section for `identity`.
pub(super) fn draw_identity(
    ui: &mut egui::Ui,
    identity: &mut Option<WorldIdentity>,
    suggestions: Suggestions,
    dirty: &mut bool,
) {
    let theme = crate::ui::theme::current(ui.ctx());
    let Some(current) = identity else {
        ui.label(
            egui::RichText::new(
                "This world has no name or banner of its own: visitors see it as its \
                 owner's.",
            )
            .small()
            .color(theme.text_weak),
        );
        if ui
            .button("Give it a name and banner")
            .on_hover_text("One suggested for this world's land - change any of it after.")
            .clicked()
        {
            *identity = Some(suggestions.nth(0));
            *dirty = true;
        }
        return;
    };
    ui.horizontal(|ui| {
        crate::ui::world_title::banner_picture(ui, Some(&current.banner), 44.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                let out = text_draft_row(
                    ui,
                    "world_name",
                    &current.name,
                    200.0,
                    "The world's name: shown as visitors arrive, over your handle. Press \
                     Enter, or click away, to apply it.",
                    world_name_refusal,
                );
                if let Some(committed) = out.committed {
                    current.name = clean_world_name(&committed);
                    *dirty = true;
                }
            });
            ui.label(
                egui::RichText::new("Shown with your handle, never in its place.")
                    .small()
                    .color(theme.text_weak),
            );
        });
    });
    color_picker(ui, "Field", &mut current.banner.field, dirty);
    color_picker(ui, "Second colour", &mut current.banner.second, dirty).on_hover_text(
        "The division's other part, and the mark's colour wherever it lies on the field's.",
    );
    ui.horizontal(|ui| {
        ui.label("Division");
        egui::ComboBox::from_id_salt("banner_division")
            .selected_text(current.banner.division.label())
            .show_ui(ui, |ui| {
                for division in BannerDivision::ALL {
                    if ui
                        .selectable_label(current.banner.division == division, division.label())
                        .clicked()
                        && current.banner.division != division
                    {
                        current.banner.division = division;
                        *dirty = true;
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Mark");
        egui::ComboBox::from_id_salt("banner_mark")
            .selected_text(current.banner.mark.label())
            .show_ui(ui, |ui| {
                for mark in BannerMark::ALL {
                    if ui
                        .selectable_label(current.banner.mark == mark, mark.label())
                        .clicked()
                        && current.banner.mark != mark
                    {
                        current.banner.mark = mark;
                        *dirty = true;
                    }
                }
            });
    });
    let mut remove = false;
    ui.horizontal(|ui| {
        let next = ui.id().with("identity_suggestion");
        // A Berlin square is named for its borough whatever is drawn.
        let suggest = if suggestions.berlin.is_some() {
            "Another banner for this world - a square of Berlin keeps its borough's name."
        } else {
            "Another name and banner of this world's land, for both."
        };
        if ui
            .button("Suggest another")
            .on_hover_text(suggest)
            .clicked()
        {
            let n = ui.data(|d| d.get_temp::<u64>(next)).unwrap_or(0) + 1;
            ui.data_mut(|d| d.insert_temp(next, n));
            *current = suggestions.nth(n);
            *dirty = true;
        }
        if ui
            .button("Remove")
            .on_hover_text("No name or banner: the world is shown as yours, as before.")
            .clicked()
        {
            remove = true;
        }
    });
    if remove {
        *identity = None;
        *dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first suggestion is the world's own seeded one; the rest differ.
    #[test]
    fn suggestions_start_with_the_worlds_own() {
        let suggestions = Suggestions {
            seed: 42,
            berlin: None,
        };
        let own =
            crate::seeded_defaults::room::identity::derive(&SceneCharacter::for_seed(42), 42, None);
        assert_eq!(suggestions.nth(0), own);
        let names: std::collections::HashSet<String> =
            (0..8).map(|n| suggestions.nth(n).name).collect();
        assert!(names.len() >= 6, "{names:?}");
        // A Berlin square's are all its borough's name, its banners various.
        let berlin = Suggestions {
            seed: 42,
            berlin: Some(geodata::berlin::Borough::Pankow),
        };
        assert!((0..8).all(|n| berlin.nth(n).name == "Pankow"));
        let banners: Vec<_> = (0..8).map(|n| berlin.nth(n).banner).collect();
        assert!(banners.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn the_header_carries_the_name() {
        assert_eq!(header(None), "Name and banner");
        let identity = WorldIdentity {
            name: "Saltholm".into(),
            ..Default::default()
        };
        assert_eq!(header(Some(&identity)), "Name and banner: Saltholm");
    }
}
