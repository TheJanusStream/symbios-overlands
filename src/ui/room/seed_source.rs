//! The seed row's source axes (#1589): where a re-rolled room's ground comes
//! from - its own terrain, or a square of real Berlin - and for Berlin how
//! big a square, in which borough; and the square lock, which keeps the
//! room's own ground across re-rolls.
//!
//! The three axes are pins like the scene's ([`ScenePins`]): a re-roll hunts
//! the first seed that draws them, and the room is then built from that seed
//! as any seeded room is. A size or a borough is a Berlin square's, so with
//! the ground pinned procedural they are refused, and say why.
//!
//! The square lock is not a pin: nothing is hunted for it. A re-roll with it
//! engaged builds the new seed's room on the ground the room has now - the
//! exact square in its record, which the owner may have drawn, moved or
//! resized in the Region source section, or its procedural terrain - and the
//! three source axes, which it overrides, are released.

use bevy_egui::egui;
use geodata::berlin::{Borough, Coverage};

use crate::pds::{GeoSource, RoomRecord};
use crate::seeded_defaults::{RegionSource, ScenePins, SizeClass, SourceKind};
use crate::ui::editable::pin_axis_row_gated;

/// Why a size or a borough cannot be pinned on procedural ground.
const NO_SQUARE: &str = "a procedural world has no square";

/// Why a source axis cannot be pinned while the square is locked.
const SQUARE_LOCKED: &str = "the square is locked";

/// Why the ground cannot be pinned procedural under a pinned size or
/// borough.
const BERLIN_PINNED: &str = "a pinned size or borough is a Berlin square's";

/// What a seed draws for the source axes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drawn {
    kind: SourceKind,
    size: SizeClass,
    borough: Option<Borough>,
}

/// The seed row's source state: the square lock, and the last seed's draw.
#[derive(Default)]
pub(crate) struct SourceRow {
    /// Whether re-rolls keep the room's own ground.
    keep_square: bool,
    /// The source the last seed shown draws, by seed: its borough costs a
    /// walk of the map, too much for every frame.
    drawn: Option<(u64, Drawn)>,
}

impl SourceRow {
    /// What `seed` draws for the source axes, memoized.
    fn drawn(&mut self, seed: u64) -> Drawn {
        match self.drawn {
            Some((at, drawn)) if at == seed => drawn,
            _ => {
                let source = RegionSource::for_seed(seed);
                let drawn = Drawn {
                    kind: source.kind,
                    size: source.size_class(),
                    borough: (source.kind == SourceKind::Berlin)
                        .then(|| source.borough())
                        .flatten(),
                };
                self.drawn = Some((seed, drawn));
                drawn
            }
        }
    }

    /// Draw the source rows into the seed row's grid (three cells each): the
    /// ground, size and borough pins `pins` holds, as `seed` rolls them, and
    /// the square lock over `record`'s own ground.
    pub(crate) fn rows(
        &mut self,
        ui: &mut egui::Ui,
        pins: &mut ScenePins,
        seed: u64,
        record: &RoomRecord,
    ) {
        let drawn = self.drawn(seed);
        let locked = self.keep_square;
        let procedural = pins.source == Some(SourceKind::Procedural);
        let (pins_size, pins_borough) = (pins.size.is_some(), pins.borough.is_some());
        let berlin = drawn.kind == SourceKind::Berlin && !procedural;
        pin_axis_row_gated(
            ui,
            "Ground",
            &SourceKind::ALL,
            SourceKind::label,
            &mut pins.source,
            rolled(locked, Some(drawn.kind)),
            |kind| {
                if locked {
                    Some(SQUARE_LOCKED.to_owned())
                } else {
                    // A pinned size or borough is a Berlin square's: on
                    // procedural ground no seed could hold it.
                    (kind == SourceKind::Procedural && (pins_size || pins_borough))
                        .then(|| BERLIN_PINNED.to_owned())
                }
            },
        );
        pin_axis_row_gated(
            ui,
            "Size",
            &SizeClass::ALL,
            SizeClass::label,
            &mut pins.size,
            rolled(locked, berlin.then_some(drawn.size)),
            refuse(locked, procedural),
        );
        pin_axis_row_gated(
            ui,
            "Borough",
            &Borough::ALL,
            Borough::name,
            &mut pins.borough,
            rolled(locked, berlin.then_some(drawn.borough).flatten()),
            refuse(locked, procedural),
        );

        // The square lock: three cells, as a pin row's.
        ui.label("Square:");
        // The lock glyphs `pin_axis_row` draws (U+1F512, U+1F513), present
        // in egui's NotoEmoji fallback (#861).
        let glyph = if locked { "\u{1F512}" } else { "\u{1F513}" };
        let hover = if locked {
            "The square is locked: re-rolls keep this world's own ground. Click to let the seed \
             draw it."
        } else {
            "Lock the square: re-rolls keep this world's own ground - this exact square of \
             Berlin, or its own terrain - and the seed draws everything else."
        };
        if ui
            .add(egui::Button::selectable(locked, glyph))
            .on_hover_text(hover)
            .clicked()
        {
            self.keep_square = !locked;
            if self.keep_square {
                // The lock decides the ground: the axes it overrides go.
                pins.source = None;
                pins.size = None;
                pins.borough = None;
            }
        }
        ui.weak(ground_summary(record.geo_source.as_ref()))
            .on_hover_text("This world's own ground, as it stands now.");
        ui.end_row();
    }

    /// The ground a re-roll from `seed` builds on: the room's own while the
    /// square is locked, else the seed's draw. The record's source is kept
    /// verbatim under the lock, another dataset's square included; this
    /// build draws only Berlin's.
    pub(crate) fn reroll_source(&self, seed: u64, record: &RoomRecord) -> RerollSource {
        if self.keep_square {
            RerollSource {
                berlin: record
                    .geo_source
                    .as_ref()
                    .and_then(GeoSource::berlin_square),
                kept: Some(record.geo_source.clone()),
            }
        } else {
            RerollSource {
                berlin: RegionSource::for_seed(seed).berlin_square(),
                kept: None,
            }
        }
    }
}

/// Why an option of a source axis is refused: every one while the square is
/// `locked`, and a Berlin axis's where the ground is pinned `procedural`.
fn refuse<T>(locked: bool, procedural: bool) -> impl Fn(T) -> Option<String> {
    move |_| {
        if locked {
            Some(SQUARE_LOCKED.to_owned())
        } else {
            procedural.then(|| NO_SQUARE.to_owned())
        }
    }
}

/// What a source axis rolls, for its row: nothing to lock while the square
/// is, and a Berlin axis has nothing on procedural ground.
fn rolled<T>(locked: bool, value: Option<T>) -> Result<T, &'static str> {
    match (locked, value) {
        (true, _) => Err(SQUARE_LOCKED),
        (false, Some(value)) => Ok(value),
        (false, None) => Err(NO_SQUARE),
    }
}

/// The ground a re-roll builds on ([`SourceRow::reroll_source`]).
pub(crate) struct RerollSource {
    /// The Berlin square to build on, or `None` for the seed's terrain.
    pub berlin: Option<geodata::GeoSquare>,
    /// The record's own source to put back after the build, where the square
    /// is locked: what it was, another dataset's included.
    pub kept: Option<Option<GeoSource>>,
}

/// "2.18 km in Mitte", or what else the room stands on.
fn ground_summary(source: Option<&GeoSource>) -> String {
    let Some(source) = source else {
        return "its own terrain".to_owned();
    };
    let Some(square) = source.berlin_square() else {
        return format!("a square of the \"{}\" dataset", source.dataset);
    };
    let (e, n) = square.centre();
    let borough = Coverage::berlin()
        .borough_at(e, n)
        .map_or("Berlin", Borough::name);
    let side = if square.size_m < 1_000 {
        format!("{} m", square.size_m)
    } else {
        format!("{:.2} km", f64::from(square.size_m) / 1_000.0)
    };
    format!("{side} of Berlin in {borough}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locked_square_keeps_the_rooms_ground_and_an_unlocked_one_draws_the_seeds() {
        let square = geodata::GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        };
        let mut record = RoomRecord::default_for_seed(0, "did:plc:row");
        record.geo_source = Some(GeoSource::berlin(square));
        let mut row = SourceRow::default();
        // Unlocked: seed 3 draws its own Berlin square, seed 0 none.
        let three = row.reroll_source(3, &record);
        assert_eq!(three.berlin, RegionSource::for_seed(3).berlin_square());
        assert!(three.berlin.is_some() && three.kept.is_none());
        assert_eq!(row.reroll_source(0, &record).berlin, None);
        // Locked: the room's own square, whatever the seed draws.
        row.keep_square = true;
        let kept = row.reroll_source(0, &record);
        assert_eq!(kept.berlin, Some(square));
        assert_eq!(kept.kept, Some(record.geo_source.clone()));
        // Another dataset's square is kept as it is, and drawn procedurally.
        record.geo_source = Some(GeoSource {
            dataset: "hamburg".into(),
            min_e: 1,
            min_n: 2,
            size_m: 3,
        });
        let other = row.reroll_source(3, &record);
        assert_eq!(other.berlin, None);
        assert_eq!(other.kept, Some(record.geo_source.clone()));
        assert_eq!(
            ground_summary(Some(&GeoSource::berlin(square))),
            "1.00 km of Berlin in Mitte"
        );
        assert_eq!(ground_summary(None), "its own terrain");
    }

    #[test]
    fn a_seeds_draw_is_read_once() {
        let mut row = SourceRow::default();
        let first = row.drawn(3);
        assert_eq!(first.kind, SourceKind::Berlin);
        assert!(first.borough.is_some());
        assert_eq!(row.drawn, Some((3, first)));
        let procedural = row.drawn(0);
        assert_eq!(procedural.kind, SourceKind::Procedural);
        assert_eq!(procedural.borough, None, "no walk for a procedural seed");
    }
}
