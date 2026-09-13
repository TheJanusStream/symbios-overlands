//! Audio bridge widget — the sovereign-side mirror of
//! [`super::material::draw_texture_bridge`].
//!
//! Renders a [`SovereignAudioConfig`] picker plus a per-variant body:
//! the shared asset-reference editor for the `Referenced` variant, and
//! for the procedural `Patch` / `Sequence` variants a compact summary
//! plus an "Edit audio…" button that pops out the full structured
//! node-graph / sequence editor shipped by `bevy_symbios_audio`'s
//! `egui` feature.
//!
//! The crate's editors operate on **native** `AudioPatch` /
//! `SequenceRecipe` and are *stateful* (canvas layout, selection, and
//! zoom persist across frames). Overlands stores the Fp-quantised
//! `Sovereign*` mirror, so the bridge keeps a native *working copy* plus
//! the editor's view-state in [`AudioEditorState`], edits that directly,
//! and writes back to the sovereign record (sanitised) only when the
//! editor reports a committed change. This avoids Fp-snapping values
//! mid-drag and losing canvas layout that a naive per-frame
//! `to_native`/`from_native` would cause.
//!
//! # What the audition plays (#1330)
//!
//! The pop-out's audition strip is the crate's [`audition_strip`], and it
//! plays what the world plays. A `Patch` is baked at the numbers the world
//! bakes that kind of slot at ([`AudioSlotKind::patch_bake`], read from the same
//! consts the bake jobs use): a world ambient patch at 22.05 kHz for 4 s, a
//! construct's (or a worn part's) at 22.05 kHz for 1 s, both looped. It used
//! to audition every patch at 44.1 kHz for 4 s, so a slow sweep swelled in
//! the editor and stuttered on the construct. A `Sequence` carries its own
//! rate everywhere. A `Referenced` clip is played host-side, flat and
//! looped, once it has been fetched ([`ReferencedAudition`]).
//!
//! # Who is listening, and when (#1337 A4/A5)
//!
//! The pop-out opens with a line saying who these edits reach. It is not
//! worked out here: [`AudioAudience`] is carried in, because only the host
//! knows — the room's edits are live to everyone present as they are made,
//! while the avatar's depend on the body kind. Everything the monitor needs
//! from the app, and the audience with it, arrives as one
//! [`AudioMonitorIo`] rather than as a handful of parameters that every
//! caller would have to keep in step.
//!
//! # What survives a close, and what does not (#1338 A6, #1333 A9)
//!
//! Closing the pop-out drops the working copy and keeps the VIEW: where the
//! owner dragged the node boxes, how far they zoomed the timeline, which
//! instrument was open, whether that slot's Auto was on. A world can hold
//! thousands of audio slots and each editor state is a couple of hash maps
//! and an undo ring, so that memory is bounded — [`REMEMBERED_SLOTS`] of
//! them, least-recently-opened dropped first — and the view and the
//! audition setting are dropped together, so a slot cannot keep one and
//! forget the other. The undo ring is NOT kept: it describes a chain of
//! values the re-seeded copy is no longer the tail of.
//!
//! While the window is open, a room undo, a revert or a re-roll changes the
//! slot under it, and `follow_record` brings the working copy back into
//! step. Without it the next commit silently re-applied what the undo had
//! removed. The swap goes through the editor's own history as a step, so a
//! Ctrl+Z inside the window goes back to what was there before the outside
//! change rather than jumping over it.

use bevy::audio::AudioSource;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_symbios_audio::ui::{
    AudioMonitor, AuditionSource, AuditionState, MonitorRequest, PatchEditorState,
    SequenceEditorState, active_instrument_canvas, audio_patch_canvas, audition_strip,
    node_kind_label, sequence_recipe_editor,
};

use crate::pds::SovereignAudioConfig;
use crate::pds::asset_reference::SovereignAssetReference;
use crate::pds::audio::{SovereignAudioPatch, SovereignSequenceRecipe};

/// What the audition strip's caption adds to "1.0 s at 22.05 kHz, looped".
const AUDITION_NOTE: &str = "as the world plays it";
/// The Sequence pop-out's left panel, holding the sequence editor: its
/// opening width, and the narrowest it can be dragged (the instruments'
/// rows and the event inspector stop fitting below it).
const SEQUENCE_PANEL_WIDTH: f32 = 420.0;
const SEQUENCE_PANEL_MIN_WIDTH: f32 = 300.0;

/// Which kind of slot an audio bridge edits, which decides how the world
/// bakes a `Patch` in it and so how the pop-out auditions one (#1330 A2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AudioSlotKind {
    /// The world's ambient bed (the Environment tab).
    #[default]
    WorldAmbient,
    /// A construct's looping emitter: a generator node in a world, or a part
    /// worn on an avatar, which is spawned as a construct.
    Construct,
}

impl AudioSlotKind {
    /// The sample rate and length, in seconds, the world bakes a `Patch` in
    /// this kind of slot at. The consts are the ones the bake jobs read, so
    /// the audition cannot drift from the world.
    pub(crate) fn patch_bake(self) -> (u32, f32) {
        match self {
            Self::WorldAmbient => (
                crate::loading::AMBIENT_PATCH_SAMPLE_RATE,
                crate::loading::AMBIENT_PATCH_SECS,
            ),
            Self::Construct => (
                crate::world_builder::spatial_audio::CONSTRUCT_PATCH_SAMPLE_RATE,
                crate::world_builder::spatial_audio::CONSTRUCT_PATCH_SECS,
            ),
        }
    }
}

/// The audio pop-out's share of an editor system: the crate's monitor, the
/// channel to it, and the app-wide mute the pop-out's banner can lift. One
/// parameter, so the two systems hosting a pop-out stay under Bevy's
/// sixteen, and so the mute is borrowed once per system.
#[derive(SystemParam)]
pub struct AudioEditorIo<'w> {
    monitor: Res<'w, AudioMonitor>,
    requests: MessageWriter<'w, MonitorRequest>,
    /// Seeks and level changes for the voice already playing — a click on
    /// the waveform, the strip's Level slider (#1338 D1, D3). A separate
    /// channel from `requests` because these steer what plays rather than
    /// replacing it.
    controls: MessageWriter<'w, bevy_symbios_audio::ui::MonitorControl>,
    muted: ResMut<'w, crate::audio_mute::AudioMuted>,
}

impl AudioEditorIo<'_> {
    /// Whether the app's sound is muted.
    pub(crate) fn muted(&self) -> bool {
        self.muted.0
    }

    /// Lend the mute to a draw that shows the banner (the Contact effects
    /// tab), written back only on a real change (see
    /// [`crate::audio_mute::lend_mute`]).
    pub(crate) fn lend_mute<R>(&mut self, draw: impl FnOnce(&mut bool) -> R) -> R {
        crate::audio_mute::lend_mute(&mut self.muted, draw)
    }
}

/// The sample rates a Sequence slot in this world may be baked at
/// (#1337 C7).
///
/// The crate's picker offers 22 050 through 96 000, which is the right
/// ladder for a host that knows nothing about its content. This one does:
/// the rate on a recipe is the rate the world bakes at and holds the
/// decoded buffer at for as long as the bed plays, mono 32-bit float, and
/// the seeded ambient bed has used 22 050 since #568 because its pad and
/// drone content sits well inside the 11 kHz Nyquist. 96 000 therefore
/// only ever bought four times the memory for the same sound — and on
/// wasm, where freed linear memory never returns to the OS, a re-bake's
/// high-water mark is permanent.
///
/// 44 100 stays, because a slot is not only ever an ambient bed: a
/// percussive or bright construct effect can have content above 11 kHz,
/// and the choice is the owner's to make with the cost written next to it.
/// A recipe already at a rate that is not here still shows it and is never
/// rewritten — see `SequenceEditorState::offered_sample_rates`.
const HOST_SAMPLE_RATES: &[u32] = &[
    crate::config::interaction::audio::WORLD_BED_SAMPLE_RATE,
    44_100,
];

/// The recipe a NEW Sequence slot starts as: the schema's default at this
/// world's bed rate (#1337 C7).
///
/// Not `SovereignSequenceRecipe::default` itself. That is a *mirror* of
/// `bevy_symbios_audio::SequenceRecipe`, held field for field to upstream's
/// own default by `pds::audio::tests::mirror_defaults_match_upstream` so a
/// value that drifts upstream is caught instead of quietly re-meaning —
/// and upstream is right to default to 44 100, which is the sensible
/// answer for a host that has not said otherwise. This world has: its beds
/// are baked and held at [`WORLD_BED_SAMPLE_RATE`], and a new slot at
/// double that cost twice the memory of the generated bed beside it for
/// content nowhere near the higher Nyquist. Which rate a slot is *born*
/// at is the editor's question, so it is answered here.
///
/// [`WORLD_BED_SAMPLE_RATE`]: crate::config::interaction::audio::WORLD_BED_SAMPLE_RATE
fn new_sequence_recipe() -> SovereignSequenceRecipe {
    SovereignSequenceRecipe {
        sample_rate: crate::config::interaction::audio::WORLD_BED_SAMPLE_RATE,
        ..Default::default()
    }
}

/// A fresh sequence editor for a slot in this world: the crate's, with
/// this host's rate choices in it.
///
/// One door for both places a `Sequence` working copy is seeded — opening
/// a slot, and a variant switch to `Sequence` — because a second one that
/// forgot would be an editor offering rates the world does not want, and
/// nothing on screen would say which of the two it was.
fn new_sequence_editor() -> SequenceEditorState {
    let mut state = SequenceEditorState::default();
    state.set_sample_rates(HOST_SAMPLE_RATES);
    state
}

/// How many slots keep their view and their audition settings between
/// openings.
///
/// Small on purpose. What this holds is a convenience — where the owner
/// dragged the node boxes, how far they zoomed the timeline, which
/// instrument was open, whether that slot's Auto was on — and an owner
/// moves between a handful of slots in a sitting, not a hundred. A world
/// can hold thousands of audio slots, and a `PatchEditorState` is a couple
/// of hash maps and an undo ring per slot, so an unbounded map is a leak
/// that grows with the session rather than with the world.
const REMEMBERED_SLOTS: usize = 8;

/// What one slot's editor looked like, kept across a close (#1338 A6).
///
/// The VIEW, not the value: the working copy is dropped on a close and
/// re-seeded on the next opening from the record or from a stranded
/// commit. What comes back is where things were, which is still true.
///
/// The undo history is NOT in here — [`SlotView::of_patch`] and
/// [`SlotView::of_sequence`] drop it on the way in. Keeping the whole
/// editor state across a close would keep the undo ring too, and that is a
/// different promise: the working copy is re-seeded on reopening, so the
/// ring describes a chain of values the copy is no longer the tail of, and
/// a Ctrl+Z after a close-and-reopen would jump to something that was
/// never on screen in this session.
enum SlotView {
    /// A `Patch` slot's canvas layout.
    Patch(Box<PatchEditorState>),
    /// A `Sequence` slot's timeline view, including each instrument
    /// canvas's own layout.
    Sequence(Box<SequenceEditorState>),
}

impl SlotView {
    /// Keep `state`'s layout, dropping its undo history.
    fn of_patch(mut state: PatchEditorState) -> Self {
        state.forget_history();
        Self::Patch(Box::new(state))
    }

    /// The same for a sequence editor.
    fn of_sequence(mut state: SequenceEditorState) -> Self {
        state.forget_history();
        Self::Sequence(Box::new(state))
    }
}

/// What each recently-opened slot looked like, and what its audition was
/// set to, oldest first — a small LRU keyed by the bridge salt.
///
/// ONE bound serves both, deliberately. These are two per-slot maps kept
/// for the same reason (what the owner left this slot as), keyed by the
/// same salt, and a slot leaving the recently-opened set should lose both
/// together: a world where a slot kept its Auto for ever but forgot its
/// node positions after eight openings would have no explanation on the
/// page for the difference. The audition map used to be an unbounded
/// `HashMap` of its own, which is the leak this replaces.
#[derive(Default)]
struct SlotMemory {
    /// Least-recently-opened first; the last entry is the slot opened most
    /// recently. At most [`REMEMBERED_SLOTS`] entries.
    slots: Vec<(String, SlotViewAndAudition)>,
}

/// One remembered slot's two halves.
#[derive(Default)]
struct SlotViewAndAudition {
    /// The editor's layout, once that slot has been closed. `None` while
    /// it is open — the live state is in [`AudioEditorState`] then — and
    /// for a slot whose strip has been drawn but whose editor has not been
    /// closed yet.
    view: Option<SlotView>,
    /// The slot's audition strip state, so a slot's Auto stays as the
    /// owner left it.
    audition: AuditionState,
}

impl SlotMemory {
    /// The entry for `salt`, made if it is not there yet, and marked as
    /// the most recently used either way.
    ///
    /// Making an entry can evict the oldest, which is the whole point of
    /// the bound.
    fn entry(&mut self, salt: &str) -> &mut SlotViewAndAudition {
        if let Some(at) = self.slots.iter().position(|(key, _)| key == salt) {
            let entry = self.slots.remove(at);
            self.slots.push(entry);
        } else {
            // Evict before pushing, so the map never exceeds the bound
            // even for one statement.
            while self.slots.len() >= REMEMBERED_SLOTS {
                self.slots.remove(0);
            }
            self.slots
                .push((salt.to_string(), SlotViewAndAudition::default()));
        }
        &mut self
            .slots
            .last_mut()
            .expect("just pushed or moved to the end")
            .1
    }

    /// Take `salt`'s remembered view, leaving its audition settings alone.
    ///
    /// Taken rather than cloned: the view goes back into the live editor,
    /// and a copy left behind would be a stale one the next opening might
    /// prefer to what the owner has since done.
    fn take_view(&mut self, salt: &str) -> Option<SlotView> {
        self.slots
            .iter_mut()
            .find(|(key, _)| key == salt)
            .and_then(|(_, entry)| entry.view.take())
    }

    /// Remember `view` for `salt`, as the most recently used slot.
    fn keep_view(&mut self, salt: &str, view: SlotView) {
        self.entry(salt).view = Some(view);
    }

    /// How many slots are remembered, for the tests that hold the bound.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether `salt` is still remembered.
    #[cfg(test)]
    fn remembers(&self, salt: &str) -> bool {
        self.slots.iter().any(|(key, _)| key == salt)
    }
}

/// Persistent state for the pop-out audio editor window. Lives on
/// [`super::RoomEditorState`]; default is "closed, no working copy".
///
/// The window edits only its native *working copy*; it never holds a
/// reference to the sovereign record. On a committed edit it stashes the
/// converted [`SovereignAudioConfig`] in [`Self::pending`] under the
/// bound [`Self::salt`]. The matching bridge call site — which *does*
/// own the live `&mut SovereignAudioConfig` for its slot — takes that
/// value the next time it runs. This keeps the window slot-agnostic, so
/// the same editor serves both the room-ambient slot and any
/// per-construct slot symmetrically.
///
/// The pending value is **delivery**, not a draft awaiting approval
/// (#1202): the crate's editors commit on every drag end, and nothing in
/// this window can decline a commit. It therefore survives the window
/// closing, the Esc ladder, and the bound slot leaving the screen — the
/// World Editor closing, a tab change, the tree selection moving — and
/// lands the next time that slot's bridge draws. It used to be a single
/// `committed` slot wiped by `close()`, and the bridge only ran while the
/// slot was on screen, so twenty minutes of node-graph work vanished on
/// the ordinary gesture of closing the window after the selection had
/// moved — while the audition kept playing the doomed working copy.
#[derive(Default)]
pub struct AudioEditorState {
    /// Whether the pop-out editor window is open.
    pub open: bool,
    /// Which slot the open editor is bound to (the bridge `salt`).
    salt: String,
    /// What the bound slot is, in the owner's words, for the title
    /// (#1202): the salt is an egui id string nobody sees elsewhere.
    label: String,
    /// Native working copy + canvas view-state for a `Patch` slot.
    patch: Option<(bevy_symbios_audio::AudioPatch, PatchEditorState)>,
    /// Native working copy + timeline view-state for a `Sequence` slot.
    sequence: Option<(bevy_symbios_audio::SequenceRecipe, SequenceEditorState)>,
    /// Committed edits awaiting pickup, keyed by slot salt. One per slot:
    /// a later commit for the same slot replaces the earlier one.
    pending: std::collections::HashMap<String, SovereignAudioConfig>,
    /// The egui frame on which the bound slot's bridge last drew, so the
    /// window can say when its edits have nowhere to land yet.
    bound_seen_frame: Option<u64>,
    /// What kind of slot the window edits, for the audition's numbers.
    kind: AudioSlotKind,
    /// What each recently-opened slot looked like and what its audition
    /// was set to — a strip shows only its own audition, so a slot opened
    /// after another must not take the other's sound and waveform for its
    /// own, and a slot reopened must come back the way it was left
    /// (#1337 A4, #1338 A6). Bounded; see [`SlotMemory`].
    slots: SlotMemory,
    /// The window was rebound to another slot while open: its next draw
    /// stops the audition the previous slot left playing.
    stop_audition: bool,
    /// Whether an editor committed an edit on the last frame. The strip is
    /// drawn above the editors, so it hears of a commit one frame later.
    committed: bool,
    /// A `Referenced` slot's fetched clip, playing for the owner to hear.
    referenced: ReferencedAudition,
    /// Whether the app's sound is muted, as the hosting system last read
    /// it, for the bridge's Referenced row.
    app_muted: bool,
    /// The bound slot's value as the editor last agreed with it: what it
    /// seeded from, or what the bridge last delivered for it. A record that
    /// stops matching this changed from somewhere else (#1333 A9).
    agreed: Option<SovereignAudioConfig>,
    /// The working copy was re-seeded from the record, so the window says
    /// so once rather than changing under the owner in silence.
    reseeded: bool,
}

impl AudioEditorState {
    /// Seed the working copy and open the window. Exactly one of `patch`
    /// / `sequence` is populated to match the variant; the other is
    /// cleared so a stale copy from a previous slot can't leak in.
    ///
    /// Seeds from a pending commit for this slot when one is stranded
    /// there, not from `audio`: the record is behind the owner's last
    /// edit until the bridge lands it, and reseeding from the record
    /// would silently roll that edit back.
    fn open_for(
        &mut self,
        audio: &SovereignAudioConfig,
        salt: &str,
        label: &str,
        kind: AudioSlotKind,
    ) {
        // "Edit audio…" on another slot rebinds an open window; what the
        // previous slot was playing is not this slot's sound.
        if self.open && self.salt != salt {
            self.stop_audition = true;
        }
        self.salt = salt.to_string();
        self.label = label.to_string();
        self.kind = kind;
        self.committed = false;
        self.patch = None;
        self.sequence = None;
        self.reseeded = false;
        // What the record holds now is what this window is in step with —
        // even when it seeds from a stranded commit, because that commit is
        // this window's own and the bridge will land it.
        self.agreed = Some(audio.clone());
        let seed = self.pending.get(salt).unwrap_or(audio);
        // The view this slot was last closed with, if it is still one of
        // the remembered few (#1338 A6). A view of the WRONG kind is not
        // used — the slot changed variant since, so a canvas layout has
        // nothing to lay out.
        let remembered = self.slots.take_view(salt);
        match seed {
            SovereignAudioConfig::Patch { patch } => {
                let view = match remembered {
                    Some(SlotView::Patch(view)) => *view,
                    _ => PatchEditorState::default(),
                };
                self.patch = Some((patch.to_native(), view));
            }
            SovereignAudioConfig::Sequence { recipe } => {
                // Through the factory on BOTH paths. A restored state
                // carries this host's sample rates because the one it is a
                // copy of came through `new_sequence_editor`; a fresh one
                // must come through it too, or a cache miss would silently
                // put 96 000 back in the picker with nothing on screen
                // saying which of the two happened (#1337 C7).
                let view = match remembered {
                    Some(SlotView::Sequence(view)) => *view,
                    _ => new_sequence_editor(),
                };
                self.sequence = Some((recipe.to_native(), view));
            }
            // Only procedural variants have an editor; others never set
            // open via the bridge button.
            _ => {}
        }
        self.open = true;
    }

    /// Stage a committed edit for the bound slot.
    fn commit(&mut self, audio: SovereignAudioConfig) {
        self.pending.insert(self.salt.clone(), audio);
    }

    /// Take `salt`'s pending commit **and record that it has landed**.
    ///
    /// The delivery half of [`Self::commit`], and the only way a commit
    /// should leave this map: the value going into the record is the value
    /// this window last agreed with, so the record coming back changed is
    /// not an outside edit to be re-seeded from (#1333 A9). The bridge
    /// takes this door, and so does
    /// [`audio_slots::land_pending`](super::audio_slots::land_pending) for
    /// the slots whose bridge is not on screen to take it (#1337 A5).
    pub(crate) fn land(&mut self, salt: &str) -> Option<SovereignAudioConfig> {
        let landed = self.pending.remove(salt)?;
        if self.salt == salt {
            self.agreed = Some(landed.clone());
        }
        Some(landed)
    }

    /// Whether anything at all is staged. The cheap first question, so the
    /// guard's walk over a record's every node is skipped in the ordinary
    /// case — which is every frame in which the owner has not just
    /// committed an audio edit.
    pub(crate) fn pending_is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// What is staged for `salt`, without taking it.
    pub(crate) fn peek_pending(&self, salt: &str) -> Option<&SovereignAudioConfig> {
        self.pending.get(salt)
    }

    /// Stage a commit for `salt` from a test, standing in for the drag end
    /// that would have staged it.
    #[cfg(test)]
    pub(crate) fn stage_for_test(&mut self, salt: &str, audio: SovereignAudioConfig) {
        self.pending.insert(salt.to_string(), audio);
    }

    /// Forget a pending commit for `salt` — the slot changed variant
    /// under it, so the edit is for a value that no longer exists.
    fn discard(&mut self, salt: &str) {
        self.pending.remove(salt);
    }

    /// Whether an edit is staged for `salt` and has not landed yet.
    pub(crate) fn has_pending(&self, salt: &str) -> bool {
        self.pending.contains_key(salt)
    }

    /// Drop the working copy and close the window. Pending commits stay:
    /// closing the window is not a way to un-commit an edit (#1202).
    pub(crate) fn close(&mut self) {
        // The working copy goes; the VIEW is kept for the next opening of
        // this slot (#1338 A6). The pair has to come apart for that: the
        // value is re-seeded from the record or from a stranded commit,
        // and where the owner put the node boxes is still true either way.
        //
        // The undo history does NOT come with it — `SlotView` drops it —
        // because the re-seeded value is not the tail of the chain the
        // ring describes. See `SlotView`.
        let salt = std::mem::take(&mut self.salt);
        if let Some((_, view)) = self.patch.take() {
            self.slots.keep_view(&salt, SlotView::of_patch(view));
        } else if let Some((_, view)) = self.sequence.take() {
            self.slots.keep_view(&salt, SlotView::of_sequence(view));
        }
        self.salt = salt;
        self.open = false;
        self.patch = None;
        self.sequence = None;
        self.agreed = None;
        self.reseeded = false;
    }

    /// Bring the working copy back into step with the record when the
    /// record changed from outside (#1333 A9). Called by the bound slot's
    /// bridge, which is the only place that holds the live value.
    ///
    /// The view state survives — node positions, zoom, which instrument is
    /// open — because the owner did not ask for the view to change; only
    /// the value did. The swap is handed to the editor's own history as a
    /// step, so a Ctrl+Z inside the window goes back to what was there
    /// before the outside change rather than jumping over it.
    fn follow_record(&mut self, audio: &SovereignAudioConfig, salt: &str) {
        if !self.open || self.salt != salt {
            return;
        }
        match reseed_decision(self.agreed.as_ref(), audio, self.has_pending(salt)) {
            Reseed::Keep | Reseed::KeepPending => {}
            Reseed::Take => {
                match audio {
                    SovereignAudioConfig::Patch { patch } => {
                        let value = patch.to_native();
                        match self.patch.as_mut() {
                            Some((copy, view)) => {
                                *copy = value;
                                view.note_external_change(copy);
                            }
                            // The slot changed variant under the window: a
                            // fresh working copy of the kind it is now, and
                            // the other kind's is dropped.
                            None => {
                                self.patch = Some((value, PatchEditorState::default()));
                                self.sequence = None;
                            }
                        }
                    }
                    SovereignAudioConfig::Sequence { recipe } => {
                        let value = recipe.to_native();
                        match self.sequence.as_mut() {
                            Some((copy, view)) => {
                                *copy = value;
                                view.note_external_change(copy);
                            }
                            None => {
                                self.sequence = Some((value, new_sequence_editor()));
                                self.patch = None;
                            }
                        }
                    }
                    // None or Referenced: there is no editor for it, so
                    // there is nothing for this window to be open on.
                    _ => {
                        self.patch = None;
                        self.sequence = None;
                        self.open = false;
                    }
                }
                self.reseeded = true;
            }
        }
        self.agreed = Some(audio.clone());
    }

    /// Whether either editor in the open pop-out took this frame's
    /// keyboard, so the room's own chords stand down (#1333).
    pub(crate) fn wants_keyboard(&self) -> bool {
        self.open
            && (self.patch.as_ref().is_some_and(|(_, v)| v.wants_keyboard())
                || self
                    .sequence
                    .as_ref()
                    .is_some_and(|(_, v)| v.wants_keyboard()))
    }

    /// Whether either editor answered this frame's Escape by clearing its
    /// selection, so the Esc ladder spends the press on that and closes
    /// nothing (#1236's one-step-per-press contract).
    pub(crate) fn took_escape(&self) -> bool {
        self.open
            && (self.patch.as_ref().is_some_and(|(_, v)| v.took_escape())
                || self.sequence.as_ref().is_some_and(|(_, v)| v.took_escape()))
    }

    /// Record whether the app's sound is muted, for this frame's bridges.
    /// The hosting system calls it before it draws them.
    pub(crate) fn set_app_muted(&mut self, muted: bool) {
        self.app_muted = muted;
    }
}

/// A `Referenced` slot's fetched clip, playing flat and looped so the owner
/// can hear it before saving (#1330 A8, prior #1197 f349). The crate's
/// monitor plays baked buffers only, so this is a plain `AudioPlayer` on
/// the resolved handle, spawned by [`sync_referenced_auditions`].
///
/// It plays only while its slot is on screen: a bridge that draws the
/// playing slot marks it, and a frame without the mark stops it, so closing
/// the editor, changing tab or selecting another node silences it.
#[derive(Default)]
pub(crate) struct ReferencedAudition {
    /// The slot whose clip should be playing, and the clip.
    want: Option<(String, Handle<AudioSource>)>,
    /// The voice playing, and the slot and clip it plays.
    voice: Option<(String, AssetId<AudioSource>, Entity)>,
    /// Whether the wanted slot's bridge drew since the last sync.
    drawn: bool,
}

impl ReferencedAudition {
    /// Whether `salt`'s clip is playing, or about to.
    fn wants(&self, salt: &str) -> bool {
        self.want.as_ref().is_some_and(|(slot, _)| slot == salt)
    }

    /// The bridge for `salt` drew this frame, with the clip its reference
    /// resolves to now, if it does. A clip that changes under the playing
    /// slot (the owner edits the URL) replaces the voice; one that stops
    /// resolving stops it.
    fn drawn(&mut self, salt: &str, clip: Option<&Handle<AudioSource>>) {
        if !self.wants(salt) {
            return;
        }
        self.drawn = true;
        match clip {
            Some(clip) => self.want = Some((salt.to_string(), clip.clone())),
            None => self.want = None,
        }
    }

    fn play(&mut self, salt: &str, clip: Handle<AudioSource>) {
        self.want = Some((salt.to_string(), clip));
        self.drawn = true;
    }

    fn stop(&mut self) {
        self.want = None;
    }

    /// Bring the voice into line with what is wanted: stop it when its slot
    /// was not drawn since the last sync, replace it when the slot or clip
    /// changed, start it when one is wanted and none plays.
    fn sync(&mut self, commands: &mut Commands) {
        if !self.drawn {
            self.want = None;
        }
        self.drawn = false;
        let wanted = self.want.as_ref().map(|(salt, clip)| (salt, clip.id()));
        let playing = self.voice.as_ref().map(|(salt, clip, _)| (salt, *clip));
        if wanted == playing {
            return;
        }
        if let Some((_, _, entity)) = self.voice.take() {
            commands.entity(entity).despawn();
        }
        if let Some((salt, clip)) = &self.want {
            let entity = commands
                .spawn((
                    AudioPlayer::new(clip.clone()),
                    PlaybackSettings::LOOP,
                    ReferencedAuditionVoice,
                ))
                .id();
            self.voice = Some((salt.clone(), clip.id(), entity));
        }
    }

    /// The voice entity, if one plays.
    fn voice(&self) -> Option<Entity> {
        self.voice.as_ref().map(|(_, _, entity)| *entity)
    }
}

/// Marks the voice of a [`ReferencedAudition`], so a voice whose editor
/// state is gone (a logout replaced it) is found and silenced.
#[derive(Component)]
pub(crate) struct ReferencedAuditionVoice;

/// Start, replace and stop the Referenced clips the World and Avatar
/// editors' bridges asked to hear, and silence any voice nothing tracks.
/// Runs every frame, in any state, so a voice never outlives the surface
/// that started it; it reads the marks the last egui pass left.
pub(crate) fn sync_referenced_auditions(
    mut commands: Commands,
    room: Option<ResMut<super::RoomEditorState>>,
    avatar: Option<ResMut<crate::ui::avatar::AvatarEditorState>>,
    voices: Query<Entity, With<ReferencedAuditionVoice>>,
) {
    let mut tracked = Vec::with_capacity(2);
    // Bypassed: the marks reset every frame, and nothing should read that
    // as the editor state changing.
    if let Some(mut room) = room {
        let referenced = &mut room.bypass_change_detection().audio_editor.referenced;
        referenced.sync(&mut commands);
        tracked.extend(referenced.voice());
    }
    if let Some(mut avatar) = avatar {
        let referenced = &mut avatar.bypass_change_detection().audio_editor.referenced;
        referenced.sync(&mut commands);
        tracked.extend(referenced.voice());
    }
    for voice in &voices {
        if !tracked.contains(&voice) {
            commands.entity(voice).despawn();
        }
    }
}

/// What the bridge should do with the pop-out's working copy when the
/// record's value for the bound slot is not the one the editor last agreed
/// with (#1333 A9).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Reseed {
    /// Leave the working copy alone.
    Keep,
    /// Leave it alone *because* the editor's own commit has not landed
    /// yet: the record is behind on purpose and the working copy is the
    /// newer of the two.
    KeepPending,
    /// The record changed from somewhere the editor cannot see — a room
    /// undo or redo, a revert, "Load from PDS", a re-rolled seed. Take the
    /// new value.
    Take,
}

/// Decide it from what the bridge can see: the value the editor last agreed
/// with, the value in the record now, and whether a commit is in flight.
///
/// Audio commits land in the live record like any other edit, so the room's
/// undo ring captures them. Nothing re-seeded the pop-out, which edits a
/// copy taken once at open — so Ctrl+Z reverted the slot in the record
/// while the window went on showing the newer values, and its next commit
/// put them back. The undo silently un-happened (#1333 A9).
pub(crate) fn reseed_decision(
    agreed: Option<&SovereignAudioConfig>,
    current: &SovereignAudioConfig,
    has_pending: bool,
) -> Reseed {
    match agreed {
        // Nothing agreed yet: the window has just opened and the caller is
        // about to record what it seeded from.
        None => Reseed::Keep,
        Some(agreed) if agreed == current => Reseed::Keep,
        // The owner's own edit is still on its way. Re-seeding here would
        // throw it away to honour a record that is deliberately behind.
        Some(_) if has_pending => Reseed::KeepPending,
        Some(_) => Reseed::Take,
    }
}

/// Variant picker + per-variant body for an audio slot.
///
/// `salt` namespaces the inner combo box so multiple bridges on the same
/// egui frame (room-ambient vs. per-construct slots) don't collide on
/// the egui id stack. `kind` is what the slot is in the world, which is
/// how its audition is baked. The pop-out editor itself is drawn separately
/// by [`draw_audio_editor_window`] so it can float above the room editor.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_audio_bridge(
    ui: &mut egui::Ui,
    audio: &mut SovereignAudioConfig,
    salt: &str,
    label: &str,
    kind: AudioSlotKind,
    dirty: &mut bool,
    editor: &mut AudioEditorState,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    // Pick up any committed edit the pop-out editor staged for this slot
    // (it edits a native working copy and writes back here, keyed by
    // salt, so the window itself stays slot-agnostic — see
    // [`AudioEditorState`]). Whether or not the window is still open or
    // still bound here: a commit is delivered, never dropped (#1202).
    if let Some(committed) = editor.land(salt) {
        *audio = committed;
        *dirty = true;
    }
    // Anything else that moved this slot — a room undo or redo, a revert, a
    // reload, a re-rolled seed — the open window follows.
    editor.follow_record(audio, salt);
    if editor.salt == salt {
        editor.bound_seen_frame = Some(ui.ctx().cumulative_frame_nr());
    }

    let prev_variant = std::mem::discriminant(&*audio);

    egui::ComboBox::from_id_salt(format!("{}_audio_ty", salt))
        .selected_text(audio.label())
        .show_ui(ui, |ui| {
            // Variant presets — switching variants resets the inner
            // payload because each variant carries different state.
            let presets: [(&'static str, SovereignAudioConfig); 4] = [
                ("None", SovereignAudioConfig::None),
                (
                    "Referenced",
                    SovereignAudioConfig::Referenced {
                        source: SovereignAssetReference::default(),
                    },
                ),
                (
                    "Patch",
                    SovereignAudioConfig::Patch {
                        patch: SovereignAudioPatch::default(),
                    },
                ),
                (
                    "Sequence",
                    SovereignAudioConfig::Sequence {
                        recipe: new_sequence_recipe(),
                    },
                ),
            ];
            for (label, preset) in presets {
                let selected = std::mem::discriminant(audio) == std::mem::discriminant(&preset);
                if ui.selectable_label(selected, label).clicked() && !selected {
                    *audio = preset;
                    *dirty = true;
                }
            }
        });

    // A variant switch invalidates any open editor bound to this slot —
    // its working copy is for the old variant. Close it so the next
    // "Edit audio…" reseeds cleanly, and drop anything it had staged.
    if std::mem::discriminant(&*audio) != prev_variant {
        editor.discard(salt);
        if editor.salt == salt {
            editor.close();
        }
    }

    match audio {
        SovereignAudioConfig::None | SovereignAudioConfig::Unknown => {}
        SovereignAudioConfig::Referenced { source } => {
            // The status row inside the reference editor is the whole of
            // #1246 f349's gap for this slot: Patch and Sequence have a
            // transport with a live error label, and the ONE variant whose
            // failure is external and likely had no verification at all.
            super::widgets::draw_asset_reference_editor(
                ui,
                source,
                salt,
                dirty,
                super::widgets::ReferenceClass::Audio,
                assets,
            );
            referenced_play_row(ui, source, salt, editor, assets);
        }
        SovereignAudioConfig::Patch { patch } => {
            draw_summary(ui, &patch_summary(patch));
            edit_button(ui, audio, salt, label, kind, editor);
        }
        SovereignAudioConfig::Sequence { recipe } => {
            draw_summary(ui, &sequence_summary(recipe));
            edit_button(ui, audio, salt, label, kind, editor);
        }
    }
}

/// Play and Stop for a `Referenced` clip, once it has been fetched (#1330
/// A8): the one kind of slot whose failure is external had no way to be
/// heard before saving. It plays flat and looped, through
/// [`ReferencedAudition`], for as long as this slot stays on screen.
fn referenced_play_row(
    ui: &mut egui::Ui,
    source: &SovereignAssetReference,
    salt: &str,
    editor: &mut AudioEditorState,
    assets: &super::assets::AssetPanel<'_>,
) {
    let clip = assets.audio_reference_clip(source);
    editor.referenced.drawn(salt, clip.as_ref());
    let Some(clip) = clip else {
        return;
    };
    let playing = editor.referenced.wants(salt);
    ui.horizontal_wrapped(|ui| {
        if playing {
            if ui
                .button("\u{23F9} Stop")
                .on_hover_text("Stop the clip")
                .clicked()
            {
                editor.referenced.stop();
            }
        } else if ui
            .button("\u{25B6} Play")
            .on_hover_text("Loop the fetched clip here, to hear it before you save")
            .clicked()
        {
            editor.referenced.play(salt, clip);
        }
        let theme = crate::ui::theme::current(ui.ctx());
        if editor.app_muted {
            ui.label(
                egui::RichText::new("Sound is muted for the whole app, so nothing will be heard.")
                    .small()
                    .color(theme.status.warn),
            );
        } else {
            ui.label(
                egui::RichText::new("Loops the clip flat, not from its place in the world.")
                    .small()
                    .color(theme.text_weak),
            );
        }
    });
}

/// "Edit audio…" button — seeds the working copy and opens the pop-out.
fn edit_button(
    ui: &mut egui::Ui,
    audio: &SovereignAudioConfig,
    salt: &str,
    label: &str,
    kind: AudioSlotKind,
    editor: &mut AudioEditorState,
) {
    let is_open = editor.open && editor.salt == salt;
    let button_text = if is_open {
        "Editing… (window open)"
    } else {
        "\u{270F} Edit audio\u{2026}"
    };
    if ui
        .add_enabled(!is_open, egui::Button::new(button_text))
        .on_hover_text("Open the structured node-graph / sequence editor")
        .on_disabled_hover_text("The audio editor is already open for this patch")
        .clicked()
    {
        editor.open_for(audio, salt, label, kind);
    }
}

/// A one-line read-only summary under the variant picker, so the owner can
/// confirm what is slotted without opening the editor.
fn draw_summary(ui: &mut egui::Ui, summary: &str) {
    ui.label(
        egui::RichText::new(summary)
            .small()
            .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
}

/// `"3 nodes · ends in Lowpass"`: how big the patch is and what is heard
/// (#1330 A8). It used to read "Sound patch — 3 nodes, output #2, seed 0",
/// the schema's words: an output id means nothing without the canvas, and
/// the seed is a detail of the noise.
fn patch_summary(patch: &SovereignAudioPatch) -> String {
    let n = patch.graph.nodes.len();
    let nodes = format!("{n} node{}", if n == 1 { "" } else { "s" });
    let output = patch
        .graph
        .nodes
        .iter()
        .find(|node| node.id == patch.graph.output);
    match output {
        Some(node) => format!(
            "{nodes} \u{00B7} ends in {}",
            node_kind_label(&node.kind.to_native())
        ),
        None => format!("{nodes} \u{00B7} its output node is missing"),
    }
}

/// `"5 instruments · 37 notes · 34 beats at 60 BPM"` (#1330 A8). It used to
/// read "Sequence — 60 BPM, 5 instruments, 5 tracks, 37 events".
fn sequence_summary(recipe: &SovereignSequenceRecipe) -> String {
    let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let notes: usize = recipe.tracks.iter().map(|t| t.events.len()).sum();
    format!(
        "{} \u{00B7} {} \u{00B7} {} beats at {} BPM",
        plural(recipe.instruments.len(), "instrument"),
        plural(notes, "note"),
        trimmed(recipe.duration_beats.0),
        trimmed(recipe.bpm.0)
    )
}

/// `34`, `34.5`, `60`: at most two decimals, the trailing zeros cut.
fn trimmed(value: f32) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Who is hearing the edits this window makes, and under what noun.
///
/// The pop-out is where the audio edits are actually made and it said
/// nothing about any of this: a committed edit lands in the live record,
/// the record is broadcast on its debounce, and every visitor re-bakes
/// their bed from it — so a slider drag is heard, half-finished, by
/// everyone in the room (#1337 A4). The World Editor's own footer has
/// carried that sentence since #1269; the window floating above it did
/// not, and the window is the one with the sliders in it.
///
/// Carried in rather than derived here, because only the host knows: the
/// room's pop-out is always [`Live`](crate::ui::editable::EditVisibility::Live),
/// while the avatar's
/// depends on the body kind — a construction-kit body IS the broadcast
/// payload, and a rigged one's rides a `serde(skip)` field, so its parts
/// reach other people only on a publish. That is the same rule the Avatar
/// window's own footer states, read from the same place.
#[derive(Clone, Copy)]
pub(crate) struct AudioAudience {
    /// Whether these edits go out as they are made.
    pub(crate) visibility: crate::ui::editable::EditVisibility,
    /// How many other people are in the world right now.
    pub(crate) peers: usize,
    /// What the owner calls the thing being edited, for the saved-only
    /// wording ("Others see your last saved avatar").
    pub(crate) noun: &'static str,
}

/// What the pop-out needs from the monitor to draw a playhead and to steer
/// what is playing (#1338 D1, D3).
///
/// A small named struct filled from what the host already holds, the way
/// [`AudioAudience`] is, rather than two more parameters:
/// `show_audio_editor_window` and `audio_editor_body` were both already at
/// eight with an allow explaining why, and a ninth and tenth would be the
/// point at which that explanation stopped being true.
pub(crate) struct AudioMonitorIo<'a> {
    /// The monitor itself — status, the last buffer, and where it is in it.
    pub(crate) monitor: &'a AudioMonitor,
    /// Where the strip's clicks and the Level slider go.
    pub(crate) controls: &'a mut Vec<bevy_symbios_audio::ui::MonitorControl>,
}

/// Render the pop-out audio editor window, if open. Edits the native
/// working copy held in `editor`; on a committed change stages the
/// converted sovereign value as the bound slot's pending edit, for its
/// bridge to pick up (see [`AudioEditorState`]).
///
/// Drawn as a top-level [`egui::Window`] sibling to the World Editor so
/// the big node canvas has room to pan/zoom. Slot-agnostic: it does not
/// touch the live record, which is why one window serves every audio
/// slot.
pub(crate) fn draw_audio_editor_window(
    ctx: &egui::Context,
    editor: &mut AudioEditorState,
    io: &mut AudioEditorIo,
    chrome: &mut crate::ui::layout::WindowChrome,
    audience: AudioAudience,
) {
    if !editor.open {
        return;
    }
    // One shared layout slot for every audio slot's pop-out: the window
    // id is salted per slot, but geometry-wise they are the same tool.
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::AudioEditor, ctx);
    let mut outbox = Vec::new();
    let mut control_outbox = Vec::new();
    let AudioEditorIo {
        monitor,
        requests,
        controls,
        muted,
    } = io;
    // The mute is lent, never borrowed through its `ResMut` for the frame:
    // it is prefs-watched, and only the banner's Unmute may move its tick.
    let shown = crate::audio_mute::lend_mute(muted, |muted| {
        show_audio_editor_window(
            ctx,
            editor,
            AudioMonitorIo {
                monitor,
                controls: &mut control_outbox,
            },
            egui::Rect::from_min_size(pos, size),
            chrome.available_rect(ctx),
            &mut outbox,
            muted,
            audience,
        )
    });
    if let Some(rect) = shown {
        chrome.remember(crate::ui::layout::UiWindow::AudioEditor, rect);
    }
    requests.write_batch(outbox);
    controls.write_batch(control_outbox);
}

/// The pop-out itself, window and body, and what the Bevy system above and
/// the layout tests below both call — so a test drives the real window, not
/// a copy of it. `default_rect` is where the window opens the first time,
/// `constrain` the rect it must stay in. Monitor requests the body makes
/// land in `requests`; `muted` is the app-wide mute, which the banner's
/// Unmute clears. Returns the window's rect when it was shown.
// Eight, one past clippy's line, and the same reason `draw_audio_bridge`
// above carries the allow: every one of them is a distinct thing the host
// holds and the window does not — the context, the state, the monitor, two
// rects, the outbox, the mute and who is listening. Bundling any pair would
// group them by arity rather than by meaning.
#[allow(clippy::too_many_arguments)]
fn show_audio_editor_window(
    ctx: &egui::Context,
    editor: &mut AudioEditorState,
    io: AudioMonitorIo<'_>,
    default_rect: egui::Rect,
    constrain: egui::Rect,
    requests: &mut Vec<MonitorRequest>,
    muted: &mut bool,
    audience: AudioAudience,
) -> Option<egui::Rect> {
    let id = editor_id(&editor.salt);
    let mut keep_open = true;
    // The bridge for the bound slot draws BEFORE this window in the same
    // system, so "seen this frame" means the slot is on screen and every
    // commit lands at once; anything else means the edits are stranded
    // until it is shown again — say so (#1202).
    let slot_on_screen = editor.bound_seen_frame == Some(ctx.cumulative_frame_nr());
    if std::mem::take(&mut editor.stop_audition) {
        requests.push(MonitorRequest::Stop);
    }
    let shown = egui::Window::new(format!("Audio Editor — {}", editor.label))
        .id(id.with("window"))
        .open(&mut keep_open)
        .resizable(true)
        .default_size(default_rect.size())
        .default_pos(default_rect.min)
        .constrain_to(constrain)
        .show(ctx, |ui| {
            audio_editor_body(
                ui,
                editor,
                AudioMonitorIo {
                    monitor: io.monitor,
                    controls: io.controls,
                },
                id,
                slot_on_screen,
                requests,
                muted,
                audience,
            );
        })
        .map(|shown| shown.response.rect);

    // Honour the window's [x] close button, and drop the working copy
    // (a fresh "Edit audio…" reseeds from the pending or committed value).
    if !keep_open {
        // Stop any audition that was looping for this slot.
        requests.push(MonitorRequest::Stop);
        editor.close();
    }
    shown
}

/// The id every id inside the pop-out for `salt` derives from.
fn editor_id(salt: &str) -> egui::Id {
    egui::Id::new(salt).with("audio_editor")
}

/// The region holding the audition strip. It has a fixed id so the layout
/// tests can read back where it was drawn (`Context::read_response`).
fn strip_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with("audition_strip")
}

/// The region holding the node canvas, fixed for the same reason.
fn canvas_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with("canvas_region")
}

/// The region holding the audience notice, fixed for the same reason: it is
/// the first line of the stack the window is, and #1339's E3 holds the
/// three of them to an order and to no overlap.
fn notice_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with("audience_notice")
}

/// Lay `add` out in a child `Ui` whose id is exactly `id`.
fn region<R>(ui: &mut egui::Ui, id: egui::Id, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope_builder(egui::UiBuilder::new().id(id), add).inner
}

/// Everything inside the pop-out's window.
#[allow(clippy::too_many_arguments)]
fn audio_editor_body(
    ui: &mut egui::Ui,
    editor: &mut AudioEditorState,
    io: AudioMonitorIo<'_>,
    id: egui::Id,
    slot_on_screen: bool,
    requests: &mut Vec<MonitorRequest>,
    muted: &mut bool,
    audience: AudioAudience,
) {
    // First line in the body, under the title: a mode indicator, not a
    // footnote. Everything below it is a control that goes out live.
    region(ui, notice_id(id), |ui| {
        crate::ui::editable::audience_notice(
            ui,
            audience.visibility,
            audience.peers,
            audience.noun,
        );
    });
    ui.add_space(4.0);
    if !slot_on_screen {
        let stranded = editor.has_pending(&editor.salt);
        ui.colored_label(
            crate::ui::theme::current(ui.ctx()).status.warn,
            if stranded {
                "Edits are kept but not applied yet — the slot this window edits \
                 is not on screen. Reselect it in its editor to apply them."
            } else {
                "The slot this window edits is not on screen. Edits are kept and \
                 apply when it is shown again."
            },
        );
        ui.add_space(4.0);
    }
    if editor.reseeded {
        // The record moved under the window and the working copy followed
        // (#1333 A9). Said once, in the theme's ok colour: nothing is
        // wrong, but the values on screen are not the ones the owner left.
        ui.colored_label(
            crate::ui::theme::current(ui.ctx()).status.ok,
            "Updated from the world editor. Undo here to go back to what was \
             showing before.",
        );
        ui.add_space(4.0);
    }
    // The audition strip comes FIRST and the canvas LAST (#1327). A canvas
    // is an `egui::Scene`, which takes all the height left in the `Ui`, so
    // whatever follows it lands below the window's content. egui's
    // `Resize` then keeps `desired_size.max(last_content_size)` and never
    // gives height back: the window grew by the strip's height every frame
    // until it met the screen edge, and the strip was never drawn. Last,
    // the canvas absorbs every change above it instead — the banner
    // appearing, the waveform arriving after the first bake.
    //
    // The crate's editors return EditorResponse { changed, rebake }; we
    // treat `rebake` (a committed edit — drag ended or a non-drag widget
    // changed) as the write-back trigger, and hand it to the audition strip
    // on the next frame, whose Auto re-bakes a playing audition after it
    // (#1330 D2). The working copy itself is mutated in place every frame,
    // so mid-drag `changed` needs no extra handling here.
    //
    // The strip auditions at the numbers the world bakes this kind of slot
    // at (#1330 A2), and the mute banner sits above it (A3).
    let committed = editor.committed;
    let salt = editor.salt.clone();
    let AudioMonitorIo { monitor, controls } = io;
    let audition = &mut editor.slots.entry(&salt).audition;
    if let Some((patch, state)) = editor.patch.as_mut() {
        let (sample_rate, secs) = editor.kind.patch_bake();
        let source = AuditionSource::patch(patch, sample_rate, secs).with_note(AUDITION_NOTE);
        region(ui, strip_id(id), |ui| {
            super::widgets::mute_banner(ui, muted);
            requests.extend(audition_strip(
                ui, monitor, audition, source, committed, *muted,
            ));
        });
        controls.extend(audition.take_controls());
        ui.separator();
        let res = region(ui, canvas_id(id), |ui| {
            audio_patch_canvas(ui, patch, state, id.with("patch"))
        });
        // "Hear this node" (#1338 D3): a COPY of the working copy with its
        // output moved, auditioned at the numbers this kind of slot bakes
        // at. The working copy is untouched, so nothing is committed and
        // nothing reaches the record or the people it is broadcast to —
        // hearing is not editing.
        //
        // Before the commit below, not after: `audition` borrows out of
        // `editor`, and `editor.commit` wants the whole of it.
        if let Some(node) = state.take_hear_node() {
            let heard = bevy_symbios_audio::ui::patch_hearing(patch, node);
            // Claimed by the strip AS THE COPY, not as the whole patch: a
            // strip recognises its own audition by a fingerprint of the
            // request, and the copy's differs from the original's. Telling
            // it the original would leave the monitor playing a sound no
            // strip admitted to, so the chip would read Idle, Stop would
            // be disabled and the waveform would not be drawn — while the
            // node was audibly looping.
            requests.push(audition.play(&AuditionSource::patch(&heard, sample_rate, secs)));
        }
        editor.committed = res.rebake;
        if res.rebake {
            let committed = SovereignAudioConfig::from_patch(patch);
            editor.commit(committed);
        }
    } else if let Some((recipe, state)) = editor.sequence.as_mut() {
        // Solo and mute pick what the audition plays: a copy with the
        // silenced tracks left out, or the recipe itself when every track
        // is heard. The recipe is not changed either way (#1338 D3).
        let heard = state.heard_recipe(recipe);
        let playing = heard.as_ref().unwrap_or(&*recipe);
        let source = AuditionSource::sequence(playing).with_note(AUDITION_NOTE);
        region(ui, strip_id(id), |ui| {
            super::widgets::mute_banner(ui, muted);
            requests.extend(audition_strip(
                ui, monitor, audition, source, committed, *muted,
            ));
        });
        controls.extend(audition.take_controls());
        // The timeline's cursor, but only while the monitor is playing
        // THIS slot's audition: one monitor serves every slot, and a
        // cursor running over a timeline whose sound is not the one in the
        // room says this recipe is sounding when it is not.
        state.set_playhead(
            audition
                .is_playing(monitor)
                .then(|| monitor.position_secs())
                .flatten(),
        );
        ui.separator();
        // The sequence editor beside the canvas rather than above it
        // (#1327 A7): a seeded recipe is ~700 px of transport, instruments,
        // timeline and inspector before any canvas, and the sanitiser's caps
        // allow far more, so it scrolls in a resizable panel and the canvas
        // keeps the full height. The panel id is global in egui, so it is
        // salted with the slot: two slots never share one panel's width.
        // `Frame::NONE` because a panel's own frame paints `panel_fill`,
        // which inside a window is a differently coloured strip (as
        // `layout::footer` found).
        let res = egui::Panel::left(id.with("sequence_panel"))
            .resizable(true)
            .default_size(SEQUENCE_PANEL_WIDTH)
            .min_size(SEQUENCE_PANEL_MIN_WIDTH)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                right: 6,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("sequence_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        sequence_recipe_editor(ui, recipe, state, id.with("seq"))
                    })
                    .inner
            })
            .inner;
        let canvas = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                left: 6,
                ..egui::Margin::ZERO
            }))
            .show(ui, |ui| {
                region(ui, canvas_id(id), |ui| {
                    active_instrument_canvas(ui, recipe, state, id.with("seq_canvas"))
                })
            })
            .inner;
        editor.committed = res.rebake || canvas.rebake;
        if res.rebake || canvas.rebake {
            let committed = SovereignAudioConfig::from_sequence(recipe);
            editor.commit(committed);
        }
    } else {
        ui.label("No editable audio in this slot.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::audio::SovereignAudioPatch;

    fn patch_slot() -> SovereignAudioConfig {
        SovereignAudioConfig::Patch {
            patch: SovereignAudioPatch::default(),
        }
    }

    /// #1202 (finding 76). Sequence: open "Edit audio…" on a construct,
    /// commit an edit (drag end), click another tree row so the bound
    /// slot's bridge stops drawing, close the pop-out, reselect the row.
    /// The commit used to live in one slot that `close()` wiped, so the
    /// edit was gone before the bridge could ever take it. It must survive
    /// the close and land when the slot's bridge next runs.
    #[test]
    fn a_commit_survives_closing_the_window_and_lands_when_its_slot_is_next_drawn() {
        let mut editor = AudioEditorState::default();
        editor.open_for(
            &patch_slot(),
            "gen_oak_1",
            "oak / Cylinder",
            AudioSlotKind::Construct,
        );
        let edited = SovereignAudioPatch {
            seed: 77,
            ..Default::default()
        };
        editor.commit(SovereignAudioConfig::Patch { patch: edited });

        // The selection moves: another slot's bridge draws and takes nothing.
        assert!(editor.land("gen_birch_0").is_none());
        // The window closes.
        editor.close();
        assert!(!editor.open);
        assert!(
            editor.has_pending("gen_oak_1"),
            "closing is not un-committing"
        );
        // The row is reselected: the bridge lands the edit.
        let landed = editor.land("gen_oak_1").expect("the edit lands");
        assert!(matches!(landed, SovereignAudioConfig::Patch { patch } if patch.seed == 77));
        assert!(!editor.has_pending("gen_oak_1"));
    }

    /// Reopening the window for a slot whose commit is still stranded
    /// seeds from that commit, not from the record — the record is behind
    /// the owner's last edit until the bridge lands it.
    #[test]
    fn reopening_a_slot_with_a_stranded_commit_seeds_from_the_commit() {
        let mut editor = AudioEditorState::default();
        let ambient = AudioSlotKind::WorldAmbient;
        editor.open_for(&patch_slot(), "environment", "Room ambient", ambient);
        let edited = SovereignAudioPatch {
            seed: 5,
            ..Default::default()
        };
        editor.commit(SovereignAudioConfig::Patch { patch: edited });
        editor.close();
        editor.open_for(&patch_slot(), "environment", "Room ambient", ambient);
        let (working, _) = editor.patch.as_ref().expect("a patch working copy");
        assert_eq!(
            working.seed, 5,
            "the working copy carries the stranded edit"
        );
        assert_eq!(editor.label, "Room ambient");
    }

    // -- A6: a layout that survives closing (#1338) ------------------------

    /// A slot's canvas layout comes back when it is reopened.
    ///
    /// THE CONTROL is the last block: before this step `close()` dropped
    /// the editor state with the working copy, and `open_for` built a
    /// fresh one, so every node the owner had arranged jumped back to the
    /// auto-layout the moment the window was closed and reopened.
    #[test]
    fn a_patch_slots_layout_survives_a_close_and_reopen() {
        let mut editor = AudioEditorState::default();
        let slot = AudioSlotKind::Construct;
        editor.open_for(&patch_slot(), "gen_oak_1", "oak", slot);

        // The owner arranges the canvas: a node moved, the view panned.
        let moved = bevy_symbios_audio::NodeId(0);
        let put_at = egui::Pos2::new(321.0, 654.0);
        {
            let (_, view) = editor.patch.as_mut().expect("a patch working copy");
            view.set_node_position(moved, put_at);
        }

        editor.close();
        editor.open_for(&patch_slot(), "gen_oak_1", "oak", slot);

        let (_, view) = editor.patch.as_ref().expect("a patch working copy");
        assert_eq!(
            view.node_position(moved),
            Some(put_at),
            "the node went back to where the auto-layout put it"
        );

        // THE CONTROL: another slot never had a layout, so it gets a
        // fresh one — the cache is per slot, not a global.
        editor.close();
        editor.open_for(&patch_slot(), "gen_birch_0", "birch", slot);
        let (_, view) = editor.patch.as_ref().expect("a patch working copy");
        assert_ne!(
            view.node_position(moved),
            Some(put_at),
            "another slot took this slot's layout"
        );
    }

    /// The same for a Sequence slot: which instrument was open, the zoom
    /// and the selection all come back.
    #[test]
    fn a_sequence_slots_view_survives_a_close_and_reopen() {
        let mut editor = AudioEditorState::default();
        let slot = AudioSlotKind::WorldAmbient;
        let value = SovereignAudioConfig::Sequence {
            recipe: SovereignSequenceRecipe::from_native(&seeded_recipe()),
        };
        editor.open_for(&value, "environment", "Room ambient", slot);
        {
            let (_, view) = editor.sequence.as_mut().expect("a sequence working copy");
            view.set_active_instrument(Some(2));
            view.set_selected_event(Some((1, 0)));
            view.set_snap(bevy_symbios_audio::ui::Snap::Eighth);
        }

        editor.close();
        editor.open_for(&value, "environment", "Room ambient", slot);

        let (_, view) = editor.sequence.as_ref().expect("a sequence working copy");
        assert_eq!(view.active_instrument(), Some(2), "the open instrument");
        assert_eq!(view.selected_event(), Some((1, 0)), "the selection");

        // THE CONTROL: a slot nobody has opened before starts fresh.
        editor.close();
        editor.open_for(&value, "another_slot", "Another", slot);
        let (_, view) = editor.sequence.as_ref().expect("a sequence working copy");
        assert_eq!(
            view.active_instrument(),
            None,
            "a new slot took another slot's view"
        );
    }

    /// THE ONE #1338 DOES NOT ASK FOR, and the one that would otherwise
    /// rot silently: a RESTORED sequence editor still offers only this
    /// host's sample rates.
    ///
    /// `new_sequence_editor` is the only door that calls `set_sample_rates`
    /// (#1337 C7). If the restore path ever CONSTRUCTS a state rather than
    /// taking the cached one — a `SequenceEditorState::default()` on a
    /// cache miss, say — the picker quietly goes back to offering 96 000,
    /// and nothing else in this file would notice.
    #[test]
    fn a_restored_sequence_editor_still_offers_only_this_worlds_rates() {
        let mut editor = AudioEditorState::default();
        let slot = AudioSlotKind::WorldAmbient;
        let value = SovereignAudioConfig::Sequence {
            recipe: SovereignSequenceRecipe::from_native(&seeded_recipe()),
        };

        // Freshly opened, through the factory.
        editor.open_for(&value, "environment", "Room ambient", slot);
        let fresh: Vec<u32> = {
            let (_, view) = editor.sequence.as_ref().expect("a working copy");
            view.sample_rates().to_vec()
        };
        assert_eq!(
            fresh, HOST_SAMPLE_RATES,
            "a fresh editor offers this host's rates"
        );
        assert!(!fresh.contains(&96_000), "96 000 is not one of them");

        // Closed and reopened, through the cache.
        editor.close();
        editor.open_for(&value, "environment", "Room ambient", slot);
        let restored: Vec<u32> = {
            let (_, view) = editor.sequence.as_ref().expect("a working copy");
            view.sample_rates().to_vec()
        };
        assert_eq!(
            restored, HOST_SAMPLE_RATES,
            "a RESTORED editor offers a different list from a fresh one"
        );

        // And after the slot has been evicted from the cache, which is the
        // path that would construct rather than restore.
        for i in 0..REMEMBERED_SLOTS + 2 {
            editor.close();
            editor.open_for(&value, &format!("filler_{i}"), "Filler", slot);
        }
        editor.close();
        assert!(
            !editor.slots.remembers("environment"),
            "the slot should have been evicted by now"
        );
        editor.open_for(&value, "environment", "Room ambient", slot);
        let after_eviction: Vec<u32> = {
            let (_, view) = editor.sequence.as_ref().expect("a working copy");
            view.sample_rates().to_vec()
        };
        assert_eq!(
            after_eviction, HOST_SAMPLE_RATES,
            "an editor rebuilt after eviction skipped the factory"
        );
    }

    /// The cache is bounded, and it is a least-recently-used one: opening
    /// a ninth slot forgets the slot nobody has touched for longest, not
    /// the one opened most recently.
    #[test]
    fn the_slot_memory_is_bounded_and_forgets_the_oldest_first() {
        let mut editor = AudioEditorState::default();
        let slot = AudioSlotKind::Construct;
        for i in 0..REMEMBERED_SLOTS {
            editor.open_for(&patch_slot(), &format!("slot_{i}"), "x", slot);
            editor.close();
        }
        assert_eq!(editor.slots.len(), REMEMBERED_SLOTS);
        assert!(editor.slots.remembers("slot_0"));

        // Touch the oldest, so it is no longer the oldest.
        editor.open_for(&patch_slot(), "slot_0", "x", slot);
        editor.close();

        // One more slot than fits: something must go, and it must be
        // slot_1 — the one nobody has touched for longest — not slot_0.
        editor.open_for(&patch_slot(), "newcomer", "x", slot);
        editor.close();
        assert_eq!(
            editor.slots.len(),
            REMEMBERED_SLOTS,
            "the memory grew past its bound"
        );
        assert!(
            editor.slots.remembers("slot_0"),
            "the recently used slot was evicted"
        );
        assert!(
            !editor.slots.remembers("slot_1"),
            "the oldest slot was kept"
        );
        assert!(editor.slots.remembers("newcomer"));
    }

    /// One bound serves the view AND the audition settings: a slot
    /// evicted loses both together, so there is never a slot remembering
    /// its Auto but not its layout.
    #[test]
    fn a_slots_audition_is_bounded_by_the_same_memory_as_its_view() {
        let mut editor = AudioEditorState::default();
        let slot = AudioSlotKind::Construct;
        // Touching a slot's audition makes an entry, exactly as opening
        // its editor does.
        editor.slots.entry("with_audition").audition.set_auto(true);
        assert_eq!(editor.slots.len(), 1);

        for i in 0..REMEMBERED_SLOTS {
            editor.open_for(&patch_slot(), &format!("slot_{i}"), "x", slot);
            editor.close();
        }
        assert_eq!(
            editor.slots.len(),
            REMEMBERED_SLOTS,
            "the audition map is not bounded by the same memory"
        );
        assert!(
            !editor.slots.remembers("with_audition"),
            "an audition entry outlived the bound its view is held to"
        );
    }

    /// Closing keeps the LAYOUT but not the UNDO RING — a deliberate
    /// difference (#1338 A6). The working copy is re-seeded on reopening,
    /// so a Ctrl+Z on a restored history would jump to a value that was
    /// never on screen in this session.
    #[test]
    fn closing_keeps_the_layout_and_drops_the_undo_history() {
        let mut editor = AudioEditorState::default();
        let slot = AudioSlotKind::Construct;
        editor.open_for(&patch_slot(), "gen_oak_1", "oak", slot);
        let moved = bevy_symbios_audio::NodeId(0);
        let put_at = egui::Pos2::new(11.0, 22.0);
        {
            let (copy, view) = editor.patch.as_mut().expect("a working copy");
            view.set_node_position(moved, put_at);
            // Something to undo: an edit recorded on the canvas's history.
            copy.seed = 99;
            view.note_external_change(copy);
        }

        editor.close();
        editor.open_for(&patch_slot(), "gen_oak_1", "oak", slot);

        let (copy, view) = editor.patch.as_mut().expect("a working copy");
        assert_eq!(
            view.node_position(moved),
            Some(put_at),
            "the layout came back"
        );
        assert!(
            !view.can_undo(),
            "the undo ring survived a close: Ctrl+Z would resurrect an edit              from the previous session"
        );
        // And the value really was re-seeded from the record, which is why
        // the old ring would have been about the wrong chain.
        assert_eq!(copy.seed, 0, "the working copy is the record's again");
    }

    /// A slot that changed variant under the cache gets a fresh editor of
    /// the kind it is now, not the remembered one of the kind it was.
    #[test]
    fn a_slot_that_changed_variant_does_not_take_the_old_kinds_view() {
        let mut editor = AudioEditorState::default();
        let slot = AudioSlotKind::WorldAmbient;
        editor.open_for(&patch_slot(), "environment", "Room ambient", slot);
        {
            let (_, view) = editor.patch.as_mut().expect("a patch working copy");
            view.set_node_position(bevy_symbios_audio::NodeId(0), egui::Pos2::new(5.0, 6.0));
        }
        editor.close();

        // The slot is a Sequence now.
        let sequence = SovereignAudioConfig::Sequence {
            recipe: SovereignSequenceRecipe::from_native(&seeded_recipe()),
        };
        editor.open_for(&sequence, "environment", "Room ambient", slot);
        assert!(editor.patch.is_none(), "the old kind's copy leaked in");
        let (_, view) = editor.sequence.as_ref().expect("a sequence working copy");
        assert_eq!(
            view.active_instrument(),
            None,
            "a fresh view for the new kind"
        );
        // Through the factory, so the rates are still this host's.
        assert_eq!(view.sample_rates(), HOST_SAMPLE_RATES);
    }

    /// Where the pop-out's parts were drawn on one frame.
    struct Landed {
        window: egui::Rect,
        notice: egui::Rect,
        strip: egui::Rect,
        canvas: egui::Rect,
    }

    /// The seeded ambient bed of a calm room: the size the Sequence arm
    /// has to fit (five instruments on five lanes, 34 beats; #1327).
    fn seeded_recipe() -> bevy_symbios_audio::SequenceRecipe {
        let mut scene = crate::seeded_defaults::scene::SceneCharacter::for_seed(3);
        scene.escalation = 0.0;
        let recipe =
            crate::seeded_defaults::room::audio::AmbientRecipe::from_scene(&scene, 3).recipe;
        assert_eq!(
            (recipe.instruments.len(), recipe.tracks.len()),
            (5, 5),
            "the seeded size this test is about"
        );
        recipe
    }

    /// A monitor that has baked something, so the strip draws its waveform.
    fn monitor(with_waveform: bool) -> AudioMonitor {
        let mut monitor = AudioMonitor::default();
        if with_waveform {
            monitor.last_samples = (0..4096).map(|i| (i as f32 * 0.05).sin()).collect();
        }
        monitor
    }

    /// The centre of the topmost widget labelled `label`, read from egui's
    /// AccessKit tree: the pop-out's widgets have no ids a test can name, so
    /// the test clicks them the way an owner does.
    fn labelled(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
        let update = output.platform_output.accesskit_update.as_ref()?;
        update
            .nodes
            .iter()
            .filter(|(_, node)| node.label() == Some(label))
            .filter_map(|(_, node)| node.bounds())
            .min_by(|a, b| a.y0.total_cmp(&b.y0))
            .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
    }

    /// Every piece of text the frame painted.
    fn painted(output: &egui::FullOutput) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    /// What one run of the pop-out did.
    struct Run {
        landed: Vec<Landed>,
        /// Every request the pop-out made, in order.
        requests: Vec<MonitorRequest>,
        /// Every seek and level change it asked of the playing voice.
        controls: Vec<bevy_symbios_audio::ui::MonitorControl>,
        /// The text painted on the last frame.
        text: Vec<String>,
    }

    /// Draw the real pop-out, [`show_audio_editor_window`], for `frames`
    /// frames on a 1920x1080 screen at the layout slot's default size, and
    /// report every frame. With `click`, the widget with that label is
    /// clicked (pressed and released) on the two frames after it is first
    /// drawn visibly, so it acts on frame 4. `muted` is the app-wide mute,
    /// which the pop-out may clear.
    fn run_pop_out(
        editor: &mut AudioEditorState,
        monitor: &AudioMonitor,
        frames: usize,
        click: Option<&str>,
        muted: &mut bool,
    ) -> Run {
        run_pop_out_seen_by(
            editor,
            monitor,
            frames,
            click,
            muted,
            AudioAudience {
                visibility: crate::ui::editable::EditVisibility::Live,
                peers: 0,
                noun: "world",
            },
        )
    }

    /// [`run_pop_out`] with the audience the window is told it has.
    fn run_pop_out_seen_by(
        editor: &mut AudioEditorState,
        monitor: &AudioMonitor,
        frames: usize,
        click: Option<&str>,
        muted: &mut bool,
        audience: AudioAudience,
    ) -> Run {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0));
        let [w, h] = crate::ui::layout::UiWindow::AudioEditor.slot().size;
        let default_rect = egui::Rect::from_min_size(egui::pos2(40.0, 60.0), egui::vec2(w, h));
        let id = editor_id(&editor.salt);
        let mut state = if click.is_some() {
            Click::Looking
        } else {
            Click::Done
        };
        let mut run = Run {
            landed: Vec::with_capacity(frames),
            requests: Vec::new(),
            controls: Vec::new(),
            text: Vec::new(),
        };
        for _ in 0..frames {
            let mut events = Vec::new();
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            state = match state {
                Click::Press(pos) => {
                    events.extend([egui::Event::PointerMoved(pos), button(pos, true)]);
                    Click::Release(pos)
                }
                Click::Release(pos) => {
                    events.push(button(pos, false));
                    Click::Done
                }
                other => other,
            };
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut this_frame = None;
            let output = ctx.run_ui(input, |ui| {
                crate::ui::theme::apply_theme(ui.ctx(), &crate::ui::theme::Theme::dark());
                let window = show_audio_editor_window(
                    ui.ctx(),
                    editor,
                    AudioMonitorIo {
                        monitor,
                        controls: &mut run.controls,
                    },
                    default_rect,
                    screen,
                    &mut run.requests,
                    muted,
                    audience,
                );
                let read = |region| ui.ctx().read_response(region).map(|r| r.rect);
                if let (Some(window), Some(notice), Some(strip), Some(canvas)) = (
                    window,
                    read(notice_id(id)),
                    read(strip_id(id)),
                    read(canvas_id(id)),
                ) {
                    this_frame = Some(Landed {
                        window,
                        notice,
                        strip,
                        canvas,
                    });
                }
            });
            run.landed
                .push(this_frame.expect("the pop-out and both its regions were drawn"));
            run.text = painted(&output);
            // Not from the first frame: a new window's first frame is an
            // invisible sizing pass (egui 0.35 `Area::begin`), and a press
            // aimed at what it laid out hits nothing.
            if state == Click::Looking
                && run.landed.len() > 1
                && let Some(pos) = click.and_then(|label| labelled(&output, label))
            {
                state = Click::Press(pos);
            }
        }
        assert_eq!(state, Click::Done, "{click:?} was never found to click");
        run
    }

    /// Where a scripted click is: press on the frame after the widget is
    /// first seen, release on the one after that.
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Click {
        Looking,
        Press(egui::Pos2),
        Release(egui::Pos2),
        Done,
    }

    /// The three things #1327 promises about one run of the pop-out.
    fn assert_the_pop_out_holds(landed: &[Landed], case: &str) {
        let settled = landed[2].window;
        for (frame, l) in landed.iter().enumerate().skip(2) {
            assert!(
                (l.window.height() - settled.height()).abs() < 0.5
                    && (l.window.width() - settled.width()).abs() < 0.5,
                "{case}: the window is {:.0}x{:.0} at frame {}, {:.0}x{:.0} at frame 3 — \
                 it grows",
                l.window.width(),
                l.window.height(),
                frame + 1,
                settled.width(),
                settled.height()
            );
        }
        let last = landed.last().expect("frames were drawn");
        assert!(
            last.window.contains_rect(last.strip),
            "{case}: the audition strip {:?} is outside the window {:?}, so it is never seen",
            last.strip,
            last.window
        );
        assert!(
            last.canvas.height() >= 250.0,
            "{case}: the canvas got {:.0} px",
            last.canvas.height()
        );
        // #1339 E3: the three are a STACK — the audience notice, then the
        // audition strip, then the canvas — each inside the window, in
        // that order, and none on top of another.
        //
        // What this adds, honestly: only the notice and the canvas being
        // inside the window at all. A REORDER cannot reach these
        // assertions, because the growth check above fires first — moving
        // the notice below the canvas was run and gave "the window is
        // 900x692 at frame 4, 900x679 at frame 3", and reordering the
        // strip and the canvas does not even compile (the audition source
        // borrows the patch the canvas edits). Nor can a shift inside the
        // vertical layout produce an overlap: the parent's cursor follows
        // the child's rect, so a region moved 60 points down takes
        // everything after it along (measured). What is left for these to
        // catch is a piece of the stack moved to an ABSOLUTE position — an
        // `egui::Area` like the crate's own picked-wire panel — which
        // nothing above would see.
        let stack = [
            ("the audience notice", last.notice),
            ("the audition strip", last.strip),
            ("the canvas", last.canvas),
        ];
        for (what, rect) in stack {
            assert!(
                last.window.contains_rect(rect),
                "{case}: {what} {rect:?} is outside the window {:?}",
                last.window
            );
        }
        for pair in stack.windows(2) {
            let [(above, a), (below, b)] = pair else {
                unreachable!("windows(2)")
            };
            assert!(
                a.bottom() <= b.top() + 0.5,
                "{case}: {above} {a:?} and {below} {b:?} overlap"
            );
        }
    }

    /// #1327 A1. A Patch slot's pop-out keeps its size and shows its
    /// audition strip, with and without a waveform. The canvas used to be
    /// drawn first: it takes all the height there is, so the strip after it
    /// landed below the window, and egui's `Resize` grew the window by the
    /// strip's height every frame until it reached the screen edge.
    ///
    /// Muted too (#1330): the banner above the strip is one more row, and
    /// the canvas, last, must absorb it.
    #[test]
    fn a_patch_pop_out_keeps_its_size_and_shows_its_audition_strip() {
        let patch = seeded_recipe().instruments[3].patch.clone();
        for (with_waveform, muted) in [(false, false), (true, false), (true, true)] {
            let mut editor = AudioEditorState::default();
            editor.open_for(
                &SovereignAudioConfig::from_patch(&patch),
                "environment",
                "Room ambient",
                AudioSlotKind::WorldAmbient,
            );
            let mut muted_now = muted;
            let run = run_pop_out(
                &mut editor,
                &monitor(with_waveform),
                40,
                None,
                &mut muted_now,
            );
            assert_the_pop_out_holds(
                &run.landed,
                &format!("patch, waveform {with_waveform}, muted {muted}"),
            );
        }
    }

    /// #1327 A1 + A7. A Sequence slot at seeded size, with an instrument
    /// open in the canvas, keeps its size and shows its audition strip, with
    /// and without a waveform. The whole sequence editor used to sit above
    /// the canvas in one column, and the strip after the canvas, so opening
    /// an instrument pushed the strip out of the window and the window
    /// grew to the screen edge.
    #[test]
    fn a_sequence_pop_out_at_seeded_size_keeps_its_size_with_an_instrument_open() {
        let recipe = seeded_recipe();
        for with_waveform in [false, true] {
            let mut editor = AudioEditorState::default();
            editor.open_for(
                &SovereignAudioConfig::from_sequence(&recipe),
                "environment",
                "Room ambient",
                AudioSlotKind::WorldAmbient,
            );
            let run = run_pop_out(
                &mut editor,
                &monitor(with_waveform),
                40,
                Some("\u{270F} Edit"),
                &mut false,
            );
            let (_, state) = editor.sequence.as_ref().expect("a sequence working copy");
            assert_eq!(
                state.active_instrument(),
                Some(0),
                "the click on the first Edit opened the first instrument"
            );
            assert_the_pop_out_holds(&run.landed, &format!("sequence, waveform {with_waveform}"));
        }
    }

    /// A variant switch on the slot is the one thing that discards a
    /// pending commit: the edit is for a value that no longer exists.
    #[test]
    fn a_variant_switch_discards_the_pending_commit_for_that_slot_only() {
        let mut editor = AudioEditorState::default();
        editor.open_for(&patch_slot(), "a", "A", AudioSlotKind::Construct);
        editor.commit(patch_slot());
        editor.open_for(&patch_slot(), "b", "B", AudioSlotKind::Construct);
        editor.commit(patch_slot());
        editor.discard("a");
        assert!(!editor.has_pending("a"));
        assert!(
            editor.has_pending("b"),
            "another slot's commit is untouched"
        );
    }

    // -----------------------------------------------------------------------
    // An audition that plays what the world plays (#1330)
    // -----------------------------------------------------------------------

    /// The sample rate and length of a world bake job for a `Patch`.
    fn patch_numbers(job: Option<gen_jobs::AudioBakeJob>) -> (u32, f32) {
        match job {
            Some(gen_jobs::AudioBakeJob::Patch {
                sample_rate,
                duration_secs,
                ..
            }) => (sample_rate, duration_secs),
            _ => panic!("a Patch slot bakes a Patch job"),
        }
    }

    /// #1330 A2. For each kind of slot, the pop-out's Audition asks for the
    /// sample rate and length the world's own bake job for that slot uses,
    /// built through the real builders. It used to be 44.1 kHz and 4 s for
    /// every patch: the world bakes an ambient patch at 22.05 kHz for 4 s
    /// and a construct's (or a worn part's) at 22.05 kHz for 1 s.
    #[test]
    fn each_slot_kind_auditions_a_patch_as_the_world_bakes_it() {
        let slot = SovereignAudioConfig::from_patch(&seeded_recipe().instruments[3].patch);
        let world = [
            (
                AudioSlotKind::WorldAmbient,
                patch_numbers(crate::loading::ambient_bake_job(&slot)),
            ),
            (
                AudioSlotKind::Construct,
                patch_numbers(crate::world_builder::spatial_audio::construct_bake_job(
                    &slot,
                )),
            ),
        ];
        assert_ne!(
            world[0].1, world[1].1,
            "the two kinds bake differently, so this test can tell them apart"
        );
        for (kind, baked) in world {
            assert_eq!(kind.patch_bake(), baked, "{kind:?}: the spec");
            let mut editor = AudioEditorState::default();
            editor.open_for(&slot, "slot", "A slot", kind);
            let run = run_pop_out(
                &mut editor,
                &AudioMonitor::default(),
                6,
                Some("\u{25B6} Audition"),
                &mut false,
            );
            let asked: Vec<(u32, f32)> = run
                .requests
                .iter()
                .filter_map(|request| match request {
                    MonitorRequest::PlayPatch {
                        sample_rate,
                        duration_secs,
                        ..
                    } => Some((*sample_rate, *duration_secs)),
                    _ => None,
                })
                .collect();
            assert_eq!(asked, [baked], "{kind:?}: what Audition asked for");
            let caption = format!(
                "{} s at 22.05 kHz, looped \u{2014} {AUDITION_NOTE}",
                if kind == AudioSlotKind::Construct {
                    "1.0"
                } else {
                    "4.0"
                }
            );
            assert!(
                run.text.contains(&caption),
                "{kind:?}: the caption says so; painted {:?}",
                run.text
            );
        }
    }

    /// A monitor that has really played `request`: the crate's plugin on a
    /// bare app, driven until the bake lands. The strip tells its own
    /// audition from another slot's by what the monitor was asked for, which
    /// only the monitor itself can record.
    fn monitor_playing(request: MonitorRequest) -> App {
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            bevy_symbios_audio::ui::AudioEditorPlugin,
        ));
        app.init_resource::<Assets<AudioSource>>();
        app.world_mut().write_message(request);
        for _ in 0..500 {
            app.update();
            if app.world().resource::<AudioMonitor>().status
                == bevy_symbios_audio::ui::MonitorStatus::Playing
            {
                return app;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("the monitor never played the request");
    }

    /// #1330. "Edit audio…" on another slot rebinds the open window. The
    /// audition the first slot left playing is stopped on the next draw, and
    /// the second slot's strip does not take that sound for its own: its
    /// chip says Idle and it draws no waveform. One strip state served every
    /// slot, so the second slot showed the first one's loop as "Playing".
    #[test]
    fn rebinding_the_window_stops_the_old_slots_audition_and_shows_none_of_it() {
        let recipe = seeded_recipe();
        let first = SovereignAudioConfig::from_patch(&recipe.instruments[0].patch);
        let second = SovereignAudioConfig::from_patch(&recipe.instruments[3].patch);
        let mut editor = AudioEditorState::default();
        editor.open_for(&first, "gen_a", "A", AudioSlotKind::Construct);
        let run = run_pop_out(
            &mut editor,
            &AudioMonitor::default(),
            6,
            Some("\u{25B6} Audition"),
            &mut false,
        );
        let request = run.requests.into_iter().next().expect("Audition asked");
        let app = monitor_playing(request);
        let monitor = app.world().resource::<AudioMonitor>();

        let run = run_pop_out(&mut editor, monitor, 4, None, &mut false);
        assert!(
            run.text.iter().any(|t| t == "Playing"),
            "the first slot's own strip shows its audition; painted {:?}",
            run.text
        );
        assert!(
            run.requests.is_empty(),
            "nothing stops it while it is shown"
        );

        editor.open_for(&second, "gen_b", "B", AudioSlotKind::Construct);
        let run = run_pop_out(&mut editor, monitor, 4, None, &mut false);
        assert!(
            matches!(run.requests.first(), Some(MonitorRequest::Stop)),
            "the rebind stops what the first slot left playing"
        );
        assert!(
            run.text.iter().any(|t| t == "Idle") && !run.text.iter().any(|t| t == "Playing"),
            "the second slot's chip is its own; painted {:?}",
            run.text
        );
        assert!(
            !run.text.iter().any(|t| t == "no signal"),
            "and it draws no waveform"
        );
    }

    /// #1330 A3. While the app's sound is muted the pop-out says so above
    /// the audition strip, and its Unmute lifts the mute; unmuted, there is
    /// no banner.
    #[test]
    fn the_pop_out_shows_the_mute_banner_while_muted_and_unmute_lifts_it() {
        let banner = "Sound is muted for the whole app, so nothing here will be heard.";
        let open = |editor: &mut AudioEditorState| {
            editor.open_for(
                &patch_slot(),
                "environment",
                "World ambient",
                AudioSlotKind::WorldAmbient,
            );
        };

        let mut editor = AudioEditorState::default();
        open(&mut editor);
        let mut muted = true;
        let run = run_pop_out(&mut editor, &AudioMonitor::default(), 4, None, &mut muted);
        assert!(
            run.text.iter().any(|t| t == banner),
            "painted {:?}",
            run.text
        );
        assert!(muted, "drawing the banner changes nothing");

        let mut editor = AudioEditorState::default();
        open(&mut editor);
        let run = run_pop_out(
            &mut editor,
            &AudioMonitor::default(),
            6,
            Some("Unmute"),
            &mut muted,
        );
        assert!(!muted, "Unmute lifts the app-wide mute");
        assert!(!run.text.iter().any(|t| t == banner), "and the banner goes");

        let mut editor = AudioEditorState::default();
        open(&mut editor);
        let run = run_pop_out(&mut editor, &AudioMonitor::default(), 4, None, &mut false);
        assert!(!run.text.iter().any(|t| t == banner));
    }

    /// #1330 A8. The summaries under the variant picker say what is slotted
    /// in words, not in the schema's ("output #2, seed 0").
    #[test]
    fn the_summaries_say_what_is_slotted_in_plain_words() {
        let recipe = seeded_recipe();
        let sequence = SovereignSequenceRecipe::from_native(&recipe);
        let notes: usize = recipe.tracks.iter().map(|t| t.events.len()).sum();
        assert_eq!(
            sequence_summary(&sequence),
            format!("5 instruments \u{00B7} {notes} notes \u{00B7} 34 beats at 60 BPM")
        );
        // A sine into a lowpass, heard from the lowpass.
        let mut lowpass_inputs = std::collections::BTreeMap::new();
        lowpass_inputs.insert(
            "in".to_string(),
            vec![bevy_symbios_audio::Connection::from_node(
                bevy_symbios_audio::NodeId(0),
            )],
        );
        let node = |id, kind, inputs| bevy_symbios_audio::GraphNode {
            id: bevy_symbios_audio::NodeId(id),
            kind,
            inputs,
        };
        let filtered = bevy_symbios_audio::AudioPatch {
            seed: 3,
            graph: bevy_symbios_audio::NodeGraph {
                nodes: vec![
                    node(
                        0,
                        bevy_symbios_audio::NodeKind::Sine(Default::default()),
                        Default::default(),
                    ),
                    node(
                        1,
                        bevy_symbios_audio::NodeKind::BiquadLowpass(Default::default()),
                        lowpass_inputs,
                    ),
                ],
                output: bevy_symbios_audio::NodeId(1),
            },
        };
        assert_eq!(
            patch_summary(&SovereignAudioPatch::from_native(&filtered)),
            "2 nodes \u{00B7} ends in Lowpass"
        );
        assert_eq!(
            patch_summary(&SovereignAudioPatch::default()),
            "1 node \u{00B7} ends in Silence"
        );
    }

    /// #1330 A8. A Referenced clip plays while its slot's bridge draws: Play
    /// starts one voice, a new clip replaces it, Stop or a frame without the
    /// slot on screen silences it.
    #[test]
    fn a_referenced_audition_plays_only_while_its_slot_is_on_screen() {
        let mut world = World::new();
        let mut clips = Assets::<AudioSource>::default();
        let clip = |clips: &mut Assets<AudioSource>| {
            clips.add(AudioSource {
                bytes: std::sync::Arc::from(Vec::new()),
            })
        };
        let (first, second) = (clip(&mut clips), clip(&mut clips));
        let voices = |world: &mut World| {
            world
                .query_filtered::<(Entity, &AudioPlayer), With<ReferencedAuditionVoice>>()
                .iter(world)
                .map(|(entity, player)| (entity, player.0.id()))
                .collect::<Vec<_>>()
        };
        let sync = |audition: &mut ReferencedAudition, world: &mut World| {
            audition.sync(&mut world.commands());
            world.flush();
        };

        let mut audition = ReferencedAudition::default();
        audition.play("env", first.clone());
        sync(&mut audition, &mut world);
        let playing = voices(&mut world);
        assert_eq!(playing.len(), 1);
        assert_eq!(playing[0].1, first.id());

        // Drawn again with the same clip: the same voice plays on.
        audition.drawn("env", Some(&first));
        sync(&mut audition, &mut world);
        assert_eq!(voices(&mut world), playing);

        // The owner edits the reference to another clip.
        audition.drawn("env", Some(&second));
        sync(&mut audition, &mut world);
        let replaced = voices(&mut world);
        assert_eq!(replaced.len(), 1);
        assert_eq!(replaced[0].1, second.id());

        // Another slot draws, this one does not: silence.
        audition.drawn("gen_oak_1", Some(&first));
        sync(&mut audition, &mut world);
        assert!(voices(&mut world).is_empty());
        assert!(!audition.wants("env"));

        // Stop is silence too.
        audition.play("env", first.clone());
        sync(&mut audition, &mut world);
        audition.drawn("env", Some(&first));
        audition.stop();
        sync(&mut audition, &mut world);
        assert!(voices(&mut world).is_empty());
    }

    // ---- step 7a (#1333 A9): the working copy follows the record --------

    /// A `Patch` slot holding one sine at `freq`, so two slots can differ
    /// by a value the owner would recognise.
    fn sine_slot(freq: f32) -> SovereignAudioConfig {
        let mut patch = bevy_symbios_audio::AudioPatch::default();
        patch.graph.nodes.push(bevy_symbios_audio::GraphNode {
            id: bevy_symbios_audio::NodeId(0),
            kind: bevy_symbios_audio::NodeKind::Sine(bevy_symbios_audio::SineOsc {
                freq_hz: freq,
                ..Default::default()
            }),
            inputs: Default::default(),
        });
        SovereignAudioConfig::Patch {
            patch: SovereignAudioPatch::from_native(&patch),
        }
    }

    /// A9: the pop-out edits a copy seeded once, at open. A room undo, a
    /// revert, a "Load from PDS" or a re-rolled seed all change the record
    /// under it, and before this the window went on showing the newer
    /// values and re-applied them on its next commit — the undo silently
    /// un-happened.
    #[test]
    fn an_outside_change_to_the_bound_slot_reseeds_the_working_copy() {
        let seeded = sine_slot(440.0);
        let reverted = sine_slot(220.0);
        assert_eq!(
            reseed_decision(Some(&seeded), &reverted, false),
            Reseed::Take,
            "the record moved and the editor did not move it"
        );
    }

    /// The editor's own commit is on its way to the record, so the record
    /// being behind is expected and the working copy is the newer of the
    /// two. Re-seeding here would undo the owner's own edit.
    #[test]
    fn a_pending_commit_keeps_the_working_copy() {
        let seeded = sine_slot(440.0);
        let record_behind = sine_slot(220.0);
        assert_eq!(
            reseed_decision(Some(&seeded), &record_behind, true),
            Reseed::KeepPending
        );
    }

    /// The bridge has just delivered the editor's own commit, so the record
    /// now holds exactly what the editor last agreed with. Nothing to do.
    #[test]
    fn the_editors_own_delivery_does_not_reseed() {
        let delivered = sine_slot(440.0);
        assert_eq!(
            reseed_decision(Some(&delivered), &delivered, false),
            Reseed::Keep
        );
    }

    /// Nothing has been agreed yet — the window has just opened — so there
    /// is nothing to compare and nothing to take.
    #[test]
    fn with_nothing_agreed_yet_there_is_nothing_to_reseed_from() {
        assert_eq!(
            reseed_decision(None, &sine_slot(440.0), false),
            Reseed::Keep
        );
        assert_eq!(reseed_decision(None, &sine_slot(440.0), true), Reseed::Keep);
    }

    /// A slot that changes variant under the editor is still an outside
    /// change: the working copy is for a value that is no longer there.
    #[test]
    fn a_variant_change_under_the_editor_is_an_outside_change() {
        assert_eq!(
            reseed_decision(Some(&sine_slot(440.0)), &SovereignAudioConfig::None, false),
            Reseed::Take
        );
    }
    // -----------------------------------------------------------------
    // Who is hearing this, and the rates this world offers (#1337 A4, C7)
    // -----------------------------------------------------------------

    /// The one honest sentence about who is hearing the edits, in the
    /// window where they are made.
    ///
    /// The wording itself is `editable::audience_line`'s and is tested
    /// there; what this asks is that the pop-out says it at all, and says
    /// the right one — the defect was a window full of live controls with
    /// nothing on it about the room.
    #[test]
    fn the_pop_out_says_who_is_hearing_the_edits() {
        let live = |peers| AudioAudience {
            visibility: crate::ui::editable::EditVisibility::Live,
            peers,
            noun: "world",
        };
        for (audience, expected) in [
            (
                live(0),
                crate::ui::editable::audience_line(
                    crate::ui::editable::EditVisibility::Live,
                    0,
                    "world",
                ),
            ),
            (
                live(3),
                crate::ui::editable::audience_line(
                    crate::ui::editable::EditVisibility::Live,
                    3,
                    "world",
                ),
            ),
            (
                AudioAudience {
                    visibility: crate::ui::editable::EditVisibility::SavedOnly,
                    peers: 2,
                    noun: "avatar",
                },
                crate::ui::editable::audience_line(
                    crate::ui::editable::EditVisibility::SavedOnly,
                    2,
                    "avatar",
                ),
            ),
        ] {
            let mut editor = AudioEditorState::default();
            editor.open_for(
                &patch_slot(),
                "environment",
                "World ambient",
                AudioSlotKind::WorldAmbient,
            );
            let run =
                run_pop_out_seen_by(&mut editor, &monitor(false), 3, None, &mut false, audience);
            assert!(
                run.text.iter().any(|t| t.contains(&expected)),
                "the pop-out does not say {expected:?}; it painted {:?}",
                run.text
            );
        }
    }

    /// The zero-peer line for a live edit says an arrival will see it, not
    /// that it is private — an empty room is luck, not privacy, because a
    /// guest is handed the unsaved state on connect. The wording is
    /// deliberate (#1269) and this pins the pop-out to it rather than to
    /// a copy of it.
    #[test]
    fn an_empty_room_is_not_called_private() {
        let mut editor = AudioEditorState::default();
        editor.open_for(
            &patch_slot(),
            "environment",
            "World ambient",
            AudioSlotKind::WorldAmbient,
        );
        let run = run_pop_out_seen_by(
            &mut editor,
            &monitor(false),
            3,
            None,
            &mut false,
            AudioAudience {
                visibility: crate::ui::editable::EditVisibility::Live,
                peers: 0,
                noun: "world",
            },
        );
        let said = run.text.join(" ");
        assert!(
            said.contains("Nobody else is here"),
            "the empty-room line is missing: {:?}",
            run.text
        );
        assert!(
            !said.to_lowercase().contains("private"),
            "the pop-out calls an empty room private: {:?}",
            run.text
        );
    }

    /// C7: a sequence editor this host opens offers the rates this world
    /// bakes at, and not the ladder up to 96 kHz.
    #[test]
    fn the_pop_out_offers_only_the_rates_this_world_bakes_at() {
        let state = new_sequence_editor();
        assert_eq!(state.sample_rates(), HOST_SAMPLE_RATES);
        assert!(
            !state.sample_rates().contains(&96_000),
            "a bed the world plays at 22 kHz can still be asked for at 96"
        );
        assert!(
            state.offered_sample_rates(96_000).contains(&96_000),
            "a recipe already at 96 000 is hidden from its own picker"
        );
    }

    /// And a NEW Sequence slot starts at the rate the seeded content uses,
    /// rather than at twice it.
    ///
    /// Asked of the preset the variant picker actually makes a slot from,
    /// not of `SovereignSequenceRecipe::default` — that one is a mirror of
    /// upstream's default and is held there on purpose
    /// (`mirror_defaults_match_upstream`), which is exactly what caught
    /// the first attempt at this.
    #[test]
    fn a_new_sequence_slot_starts_at_the_worlds_rate() {
        let recipe = new_sequence_recipe();
        assert_ne!(
            recipe.sample_rate,
            crate::pds::audio::SovereignSequenceRecipe::default().sample_rate,
            "the mirror's default has moved; this test is no longer asking anything"
        );
        assert_eq!(
            recipe.sample_rate,
            crate::config::interaction::audio::WORLD_BED_SAMPLE_RATE,
            "a hand-authored slot and the seeded bed disagree about the rate \
             this world plays beds at"
        );
        assert!(
            HOST_SAMPLE_RATES.contains(&recipe.sample_rate),
            "a new slot opens at a rate its own picker does not offer"
        );
    }
}
