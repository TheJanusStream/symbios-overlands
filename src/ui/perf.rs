//! Per-frame cost that scales with authored content (#1270), and the
//! instruments that hold it down.
//!
//! ## Count the work, do not time it
//!
//! The editors got slower in proportion to how much the owner had built,
//! which is exactly backwards: an open panel should cost what it shows.
//! Six such costs were found and fixed under #1270, and none of them had
//! an instrument. The repo has no `benches/`, no perf-test idiom, and the
//! diagnostics suite's `runtime.frame_time.ms` is a 1 Hz EMA — it cannot
//! tell you whether a per-frame allocation went away.
//!
//! A wall-clock assertion would be flaky and there is no harness for one.
//! So every guard under #1270 counts the WORK instead: how many times a
//! baseline was serialised, how many tree nodes were built, how many rows
//! a list asked to draw, how many bytes an undo ring holds. Each of those
//! is deterministic and each is the thing the fix is actually about.
//!
//! And each guard is written as a PAIR — the old shape asserted to do the
//! bad thing, right beside the new one asserted not to. Without that
//! pairing you have a test that passes on both versions, which is no
//! check at all (the #87 rule: ask what the failing case looks like; if it
//! looks like the passing case there is nothing being measured). The
//! pairing lives with each fix:
//!
//! * f121 — the change-tick pairing in this module's tests, plus the
//!   source scan
//!   `ui::fonts::glyph_coverage_tests::every_panel_flag_write_is_guarded`.
//! * f273 / f418 — [`LiveValueCache`], whose `recomputes()` counter is the
//!   measurement; the tests drive a live record through a frame loop.
//! * f419 — `ui::room::generators::tree`'s node counter, run headless with
//!   everything collapsed.
//! * f420 — `ui::room::placements`' row counter, run headless.
//! * f417 — `ui::undo`'s byte accounting.

use bevy::ecs::change_detection::Tick;

/// A `serde_json::Value` of a live record, rebuilt only when the record
/// could have changed (#1270 f418, f273).
///
/// The World Editor's footer derives `dirty` by serialising the whole
/// live record and deep-comparing it against a cached baseline. #674
/// cached the two BASELINES — the stored record and the seeded default —
/// and left the live side to run per frame, with the comment saying so
/// out loud: "an open panel pays for ONE live-record serialization per
/// frame". At the record's own caps (256 generators of up to 1024 nodes,
/// 16 KiB of L-system source each) that one is a multi-megabyte `Value`
/// tree allocated, walked and dropped sixty times a second. The Avatar
/// editor never got even that: it paid four whole-record serialisations
/// plus a deep clone and a `PartialEq` walk per frame.
///
/// ## Why the key is a tick AND a flag
///
/// A tick alone is not enough, and this is the part that is easy to get
/// wrong. Both editors route their widget writes through
/// `bypass_change_detection()` on purpose (a `&mut` through the `ResMut`
/// would broadcast `RoomStateUpdate` to every peer at frame rate), and
/// they call `set_changed()` only when a ~0.25 s debounce drains. So
/// during a slider drag the record's CONTENT changes every frame while
/// its change tick does not move at all: a tick-keyed cache would report
/// the Save row clean for the whole drag.
///
/// So the cache takes both. The tick covers everything that reaches the
/// record from outside the editor — the 3D gizmo, an inventory drop, a
/// peer's live-sync update, an undo restore, a fresh fetch. [`touch`] is
/// what the editor calls when one of its own widgets reported a change,
/// which is a fact only the editor knows.
///
/// [`touch`]: LiveValueCache::touch
#[derive(Default)]
pub struct LiveValueCache {
    cached: Option<(Tick, Option<serde_json::Value>)>,
    /// Set by [`LiveValueCache::touch`], cleared by the next rebuild.
    dirty: bool,
    /// How many times the record has actually been serialised. The
    /// instrument: a guard that asserts a cost went away has to be able
    /// to see the cost. Cheap enough (one `u64` add on a rebuild) to
    /// carry in release, and a diagnostics readout could show it.
    recomputes: u64,
}

impl LiveValueCache {
    /// Record that a widget in this editor just mutated the live record
    /// through `bypass_change_detection`, so the cached value is stale
    /// even though the resource's tick has not moved.
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// Drop the cached value outright — for a change of subject rather
    /// than of content (a room transition, a logout, a different DID),
    /// where the next tick may compare equal to a tick from before.
    pub fn clear(&mut self) {
        self.cached = None;
        self.dirty = false;
    }

    /// The live record's serialised form, rebuilt only if `tick` has
    /// moved or [`Self::touch`] was called since the last rebuild.
    ///
    /// `tick` is the resource's `last_changed()`, not `is_changed()` —
    /// the change flag is consumed on frames where the caller
    /// early-returns, which would leave a stale value behind. Same
    /// reasoning as `RoomEditorState::stored_baseline`'s (#674).
    pub fn value<R: serde::Serialize>(
        &mut self,
        tick: Tick,
        record: &R,
    ) -> &Option<serde_json::Value> {
        let stale = match self.cached.as_ref() {
            Some((cached_tick, _)) => self.dirty || *cached_tick != tick,
            None => true,
        };
        if stale {
            self.recomputes += 1;
            self.dirty = false;
            self.cached = Some((tick, serde_json::to_value(record).ok()));
        }
        &self.cached.as_ref().expect("just populated").1
    }

    /// How many serialisations this cache has paid for. The measurement
    /// #1270's guards assert on; never read by the UI.
    pub fn recomputes(&self) -> u64 {
        self.recomputes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::toolbar::UiPanels;

    /// The f121 pairing, on a real `World`: writing a window's open flag
    /// back unconditionally marks `UiPanels` changed every frame — even
    /// when the value written is the one already there.
    ///
    /// That last clause is the whole finding. Bevy's `ResMut::deref_mut`
    /// stamps the change tick on ACCESS, not on difference; it never
    /// compares. So the Catalogue's `panels.catalogue = open;` dirtied the
    /// resource on all sixty frames a second whether or not the window was
    /// open, and `prefs::save_prefs_when_changed` ORs `panels.is_changed()`
    /// into a 1.0 s trailing debounce that therefore never went quiet —
    /// a full prefs serialise-and-write about once a second, all session,
    /// for the whole app and not only the Catalogue.
    ///
    /// Both halves run the same hundred frames through the same world, so
    /// the only difference measured is the write.
    #[test]
    fn the_unguarded_writeback_dirties_uipanels_on_every_frame() {
        use bevy::prelude::*;

        // Frame f's local `open`, as `egui::Window::open` would leave it:
        // closed for fifty frames, then open, then the close click on the last.
        fn open_at(frame: usize) -> bool {
            (50..99).contains(&frame)
        }

        /// How many of `frames` see `UiPanels` as changed, when the flag
        /// is written back by `write`.
        fn dirty_frames(write: fn(&mut Mut<UiPanels>, bool)) -> usize {
            let mut world = World::new();
            world.insert_resource(UiPanels::default());
            // `catalogue` starts false and the run leaves it false, so any
            // change tick observed is the write and nothing else.
            let mut dirty = 0;
            for frame in 0..100 {
                // What `App::update` does at a frame boundary: everything
                // that changed last frame stops reading as changed.
                world.clear_trackers();
                world.increment_change_tick();
                {
                    let mut panels = world.resource_mut::<UiPanels>();
                    // The system under test: `panels.catalogue` is copied
                    // into a local, egui does its thing, the local comes
                    // back.
                    let was = panels.catalogue;
                    let open = was && open_at(frame);
                    write(&mut panels, open);
                }
                if world.resource_ref::<UiPanels>().is_changed() {
                    dirty += 1;
                }
            }
            dirty
        }

        let shipped = dirty_frames(|panels, open| {
            // src/ui/catalogue.rs as it stood at #1270 (f121).
            panels.catalogue = open;
        });
        let guarded = dirty_frames(|panels, open| {
            // The idiom the other eight windows carry (#879).
            if panels.catalogue && !open {
                panels.catalogue = false;
            }
        });

        assert_eq!(
            shipped, 100,
            "the control has to do the bad thing, or there is nothing to \
             compare against — an unconditional write is a change tick per \
             frame even though `catalogue` is false throughout"
        );
        assert_eq!(
            guarded, 0,
            "the guarded form never touches the resource on a closed window"
        );
    }

    /// The other half of being correct: the guarded write still closes the
    /// window, so the close button is not inert.
    #[test]
    fn the_guarded_writeback_still_closes_the_window() {
        use bevy::prelude::*;

        let mut world = World::new();
        world.insert_resource(UiPanels::default());
        world.resource_mut::<UiPanels>().catalogue = true;

        let mut dirty = 0;
        for frame in 0..10 {
            world.clear_trackers();
            world.increment_change_tick();
            {
                let mut panels = world.resource_mut::<UiPanels>();
                // egui clears the local on the frame the window is closed.
                let open = panels.catalogue && frame != 5;
                if panels.catalogue && !open {
                    panels.catalogue = false;
                }
            }
            if world.resource_ref::<UiPanels>().is_changed() {
                dirty += 1;
            }
        }
        assert_eq!(dirty, 1, "one write, on the frame the window was closed");
        assert!(
            !world.resource_ref::<UiPanels>().catalogue,
            "and it actually closed the window"
        );
    }

    #[derive(serde::Serialize, Clone)]
    struct Record {
        name: String,
        nodes: Vec<u32>,
    }

    fn record() -> Record {
        Record {
            name: "oak".into(),
            nodes: (0..64).collect(),
        }
    }

    /// The f418 / f273 pairing. The shipped shape serialises once per
    /// frame; the cache serialises once per change.
    #[test]
    fn a_quiet_record_is_serialised_once_not_once_per_frame() {
        let mut cache = LiveValueCache::default();
        let record = record();
        let tick = Tick::new(7);

        // Sixty frames of an open editor with nobody touching anything —
        // the case the whole finding is about.
        for _ in 0..60 {
            let _ = cache.value(tick, &record);
        }
        assert_eq!(
            cache.recomputes(),
            1,
            "an open panel on a quiet record pays for one serialisation, ever"
        );

        // The control: what the shipped code did is `to_value` per frame,
        // and that count is the frame count by construction.
        let shipped: usize = (0..60)
            .map(|_| serde_json::to_value(&record).is_ok() as usize)
            .sum();
        assert_eq!(shipped, 60, "the shape being replaced pays 60");
    }

    /// An out-of-band edit moves the resource's tick, and that is what the
    /// cache watches.
    #[test]
    fn a_moved_tick_rebuilds_exactly_once() {
        let mut cache = LiveValueCache::default();
        let mut record = record();

        for _ in 0..10 {
            let _ = cache.value(Tick::new(3), &record);
        }
        record.name = "birch".into();
        for _ in 0..10 {
            let _ = cache.value(Tick::new(4), &record);
        }

        assert_eq!(cache.recomputes(), 2);
        assert_eq!(
            cache.value(Tick::new(4), &record).as_ref().unwrap()["name"],
            "birch",
            "and the value it returns is the new one"
        );
    }

    /// The trap this type exists to avoid: an editor writing through
    /// `bypass_change_detection` does NOT move the tick, so a tick-keyed
    /// cache alone would report a dragged slider clean for the whole drag.
    #[test]
    fn a_bypassed_edit_is_seen_even_though_the_tick_did_not_move() {
        let mut cache = LiveValueCache::default();
        let mut record = record();
        let tick = Tick::new(11);

        assert_eq!(cache.value(tick, &record).as_ref().unwrap()["name"], "oak");

        // A slider drag: the record changes, the tick does not, because
        // the debounce has not drained yet.
        record.name = "willow".into();
        assert_eq!(
            cache.value(tick, &record).as_ref().unwrap()["name"],
            "oak",
            "without the touch, a tick-keyed cache is stale — this is the \
             control for the flag half"
        );

        cache.touch();
        assert_eq!(
            cache.value(tick, &record).as_ref().unwrap()["name"],
            "willow"
        );

        let before = cache.recomputes();
        for _ in 0..30 {
            let _ = cache.value(tick, &record);
        }
        assert_eq!(
            cache.recomputes(),
            before,
            "one touch rebuilds once, not once per following frame"
        );
    }

    /// A change of subject, where the next tick may compare equal to one
    /// from before the change.
    #[test]
    fn clear_forgets_the_subject() {
        let mut cache = LiveValueCache::default();
        let record = record();
        let tick = Tick::new(2);
        let _ = cache.value(tick, &record);
        cache.clear();
        let _ = cache.value(tick, &record);
        assert_eq!(cache.recomputes(), 2);
    }
}
