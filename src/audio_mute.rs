//! Master mute — a single app-wide toggle that silences *all* audio:
//! the room's ambient bed, every spatial-construct loop, and the
//! transient contact / footstep one-shots.
//!
//! Bevy's [`GlobalVolume`] is the wrong tool
//! here: it is only read when a sink is *created* and "does not affect
//! already playing audio", and a sink born while it is silent stores a
//! zero volume that a later unmute can't recover. So the master mute is
//! driven entirely through per-sink
//! [`AudioSinkPlayback::mute`]/[`unmute`](bevy::audio::AudioSinkPlayback),
//! which stash and restore each sink's real volume losslessly.
//!
//! [`reconcile_sink_mute`] runs every frame and brings every live
//! [`AudioSink`] / [`SpatialAudioSink`] into agreement with
//! [`AudioMuted`] — so sinks that spawn *after* a toggle (a new ambient
//! bake, a fresh footstep) are caught within a frame too. The most
//! prominent loop, the ambient bed, additionally spawns pre-muted (see
//! `loading::ambient`) so launching muted never leaks even a one-frame
//! blip.

use bevy::audio::{AudioSink, AudioSinkPlayback, SpatialAudioSink};
use bevy::prelude::*;

/// App-wide master-mute flag. `true` = everything silent.
///
/// Defaults to **muted** so the app launches silent (the owner opts in
/// to sound via the toolbar). Deliberately *not* reset on logout — it's
/// an app-level preference, not session state, so a relog keeps the
/// owner's choice.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioMuted(pub bool);

impl Default for AudioMuted {
    fn default() -> Self {
        Self(true)
    }
}

pub struct AudioMutePlugin;

impl Plugin for AudioMutePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioMuted>()
            .add_systems(Update, reconcile_sink_mute);
    }
}

/// Drive every live sink to match [`AudioMuted`]. Cheap: it iterates a
/// handful of sinks and only touches one (marking it changed) on a genuine
/// state flip, so steady-state frames do no work. Running unconditionally
/// (not state-gated) means a sink spawned in any state is reconciled.
fn reconcile_sink_mute(
    muted: Res<AudioMuted>,
    mut sinks: Query<&mut AudioSink>,
    mut spatial_sinks: Query<&mut SpatialAudioSink>,
) {
    let want = muted.0;
    for mut sink in &mut sinks {
        if want && !sink.is_muted() {
            sink.mute();
        } else if !want && sink.is_muted() {
            sink.unmute();
        }
    }
    for mut sink in &mut spatial_sinks {
        if want && !sink.is_muted() {
            sink.mute();
        } else if !want && sink.is_muted() {
            sink.unmute();
        }
    }
}
