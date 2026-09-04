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
//!
//! It is also the second reason a sink can be silent (#1219 f324):
//! [`SilencedByMute`] carries the entities belonging to a peer the user has
//! muted. Per-peer silencing HAS to go through this system rather than
//! calling `mute()` on those sinks directly, because the reconciler runs
//! every frame and would unmute them again the moment the master toggle is
//! off. One system owns sink mute state, exactly as one system owns a
//! peer's `Visibility`.

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

/// Entities whose audio must be silent for a reason other than the master
/// toggle: they belong to a peer the user has muted (#1219 f324).
///
/// Written by `network::presence::sync_mute_audio`, which walks the
/// descendants of every muted peer — a peer's spatial emitters live on the
/// generator nodes under their chassis, and a hidden body's
/// `PlaybackMode::Loop` emitter kept playing, so muting a harasser running a
/// screaming avatar made them WORSE: audible, invisible, and impossible to
/// aim at.
#[derive(Resource, Default, Debug)]
pub struct SilencedByMute(pub bevy::platform::collections::HashSet<Entity>);

pub struct AudioMutePlugin;

impl Plugin for AudioMutePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioMuted>()
            .init_resource::<SilencedByMute>()
            .add_systems(Update, reconcile_sink_mute);
    }
}

/// Whether a given sink should be silent right now.
///
/// Pure, so the two reasons can be tested apart: the app-wide toggle, and
/// this sink belonging to a muted peer. Unmuting one must not un-silence the
/// other — a user who turns the master mute off does not thereby un-mute the
/// person they blocked.
pub fn sink_is_silenced(master_muted: bool, silenced: &SilencedByMute, sink: Entity) -> bool {
    master_muted || silenced.0.contains(&sink)
}

/// Drive every live sink to match [`AudioMuted`]. Cheap: it iterates a
/// handful of sinks and only touches one (marking it changed) on a genuine
/// state flip, so steady-state frames do no work. Running unconditionally
/// (not state-gated) means a sink spawned in any state is reconciled.
fn reconcile_sink_mute(
    muted: Res<AudioMuted>,
    silenced: Res<SilencedByMute>,
    mut sinks: Query<(Entity, &mut AudioSink)>,
    mut spatial_sinks: Query<(Entity, &mut SpatialAudioSink)>,
) {
    for (entity, mut sink) in &mut sinks {
        let want = sink_is_silenced(muted.0, &silenced, entity);
        if want && !sink.is_muted() {
            sink.mute();
        } else if !want && sink.is_muted() {
            sink.unmute();
        }
    }
    for (entity, mut sink) in &mut spatial_sinks {
        let want = sink_is_silenced(muted.0, &silenced, entity);
        if want && !sink.is_muted() {
            sink.mute();
        } else if !want && sink.is_muted() {
            sink.unmute();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1219 f324. The sequence: a harasser puts a `PlaybackMode::Loop`
    /// emitter on their avatar, the user ticks Mute — whose tooltip promises
    /// it "Hides their avatar, chat, audio and gift offers" — and the hum
    /// keeps playing from a body they can no longer see or aim at. The two
    /// reasons a sink is silent are independent: turning the master mute off
    /// must not un-mute the person you blocked.
    #[test]
    fn the_master_toggle_and_a_peer_mute_silence_independently() {
        let mut world = World::new();
        let theirs = world.spawn_empty().id();
        let mine = world.spawn_empty().id();
        let mut silenced = SilencedByMute::default();
        silenced.0.insert(theirs);

        assert!(sink_is_silenced(false, &silenced, theirs));
        assert!(
            !sink_is_silenced(false, &silenced, mine),
            "muting one person is not muting the room"
        );
        assert!(
            sink_is_silenced(true, &silenced, mine),
            "master mute covers everything"
        );
        assert!(
            sink_is_silenced(true, &silenced, theirs),
            "and does not cancel the peer mute underneath it"
        );

        silenced.0.clear();
        assert!(
            !sink_is_silenced(false, &silenced, theirs),
            "unmuting the person restores their audio"
        );
    }
}
