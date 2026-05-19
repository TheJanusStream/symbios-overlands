//! Worked example of a [`ContactAudioHook`] (Phase 4 remainder, #246).
//!
//! Demonstrates that the audio fanout channel can be driven from
//! outside the interaction crate **without modifying any interaction
//! internals**: this whole module is just a `ContactAudioHook` impl
//! and is wired purely by inserting a resource —
//!
//! ```ignore
//! use crate::interaction::audio::ContactAudioDispatch;
//! use crate::interaction::audio_example::FootstepAudioHook;
//!
//! // Anywhere with `&mut App` (a downstream plugin, main, a test):
//! app.insert_resource(ContactAudioDispatch::with(FootstepAudioHook::default()));
//! ```
//!
//! The example is intentionally backend-free — it records what it
//! *would* play instead of pulling in `bevy_audio` — so it documents
//! the seam without dictating an audio stack. A real implementation
//! swaps the `// would play …` body for an `AudioBundle` spawn, an
//! FMOD event, a mixer call, etc.

use super::audio::ContactAudioHook;
use super::contact::{ContactPhase, ContactSample, SurfaceContact};

/// Example hook: turns terrain `Enter` contacts into "footstep" cues.
///
/// Counts how many it has fired and remembers the loudest (highest
/// intensity) so a test — or a curious dev — can observe the channel
/// working end to end.
#[derive(Default)]
pub struct FootstepAudioHook {
    /// Number of footstep cues "played" so far.
    pub footsteps: u64,
    /// Intensity of the loudest cue seen (0 if none yet).
    pub loudest: f32,
}

impl FootstepAudioHook {
    /// Decide whether a sample should trigger a footstep cue. A footfall
    /// is the first frame of ground contact (`Enter` on `Terrain`).
    fn is_footstep(sample: &ContactSample) -> bool {
        sample.phase == ContactPhase::Enter
            && matches!(sample.surface, SurfaceContact::Terrain { .. })
    }
}

impl ContactAudioHook for FootstepAudioHook {
    fn on_contacts(&mut self, samples: &[ContactSample]) {
        for s in samples.iter().filter(|s| Self::is_footstep(s)) {
            // A real impl would play a footstep here, e.g.:
            //   commands.spawn(AudioPlayer::new(self.clip.clone()));
            // scaling volume by `s.intensity`. We only record it so the
            // example stays audio-backend-free.
            self.footsteps += 1;
            self.loudest = self.loudest.max(s.intensity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    fn terrain(phase: ContactPhase, intensity: f32) -> ContactSample {
        ContactSample {
            avatar: Entity::PLACEHOLDER,
            world_pos: Vec3::ZERO,
            world_vel: Vec3::ZERO,
            footprint_radius: 0.5,
            surface: SurfaceContact::Terrain {
                material_blend: [1.0, 0.0, 0.0, 0.0],
                normal: Vec3::Y,
            },
            intensity,
            phase,
        }
    }

    fn water(phase: ContactPhase) -> ContactSample {
        ContactSample {
            surface: SurfaceContact::Water {
                plane_idx: 0,
                depth: 1.0,
                flow_dir: Vec2::ZERO,
            },
            ..terrain(phase, 1.0)
        }
    }

    #[test]
    fn fires_only_on_terrain_enter_and_tracks_loudest() {
        let mut hook = FootstepAudioHook::default();
        hook.on_contacts(&[
            terrain(ContactPhase::Enter, 0.4), // footstep
            terrain(ContactPhase::Dwell, 0.9), // not a footfall
            terrain(ContactPhase::Enter, 0.7), // footstep (loudest)
            water(ContactPhase::Enter),        // wrong surface
        ]);
        assert_eq!(hook.footsteps, 2);
        assert!((hook.loudest - 0.7).abs() < 1e-6);
    }
}
