//! Audio fanout hook — optional consumer channel D of the interaction
//! framework (Phase 4 remainder, #246).
//!
//! Exposes the per-frame [`ContactSample`] stream to downstream code
//! (a game-specific SFX system, or a separate audio crate) **without
//! coupling the interaction crate to `bevy_audio`** or any concrete
//! audio backend. The seam is a single trait + a resource holding an
//! optional boxed implementation:
//!
//! - Default: no hook installed → [`dispatch_contact_audio`] early-
//!   returns, zero cost.
//! - To plug in: a downstream `App` inserts a
//!   [`ContactAudioDispatch`] carrying its own [`ContactAudioHook`]
//!   impl. Nothing in `interaction` needs to change — see
//!   [`super::audio_example`] for a complete worked example.

use bevy::prelude::*;

use crate::state::AppState;

use super::contact::{AvatarContacts, ContactSample};

/// Implemented by downstream code to receive contact events. Kept
/// deliberately minimal and backend-agnostic: it is handed this
/// frame's finished [`ContactSample`]s (already world-space, with
/// phase/intensity) and decides what, if anything, to play. No Bevy
/// audio types appear here, so an implementor can drive `bevy_audio`,
/// a custom mixer, FMOD, or just logging.
pub trait ContactAudioHook: Send + Sync + 'static {
    /// Called once per frame with every contact sample produced this
    /// frame (empty slice if none). Implementors typically filter by
    /// [`ContactSample::phase`] (e.g. a one-shot on `Enter`).
    fn on_contacts(&mut self, samples: &[ContactSample]);
}

/// Holds the optional installed [`ContactAudioHook`]. Defaults to
/// `None` (no audio fanout). A downstream app installs a hook with
/// [`ContactAudioDispatch::with`].
#[derive(Resource, Default)]
pub struct ContactAudioDispatch {
    hook: Option<Box<dyn ContactAudioHook>>,
}

impl ContactAudioDispatch {
    /// Build a dispatch resource that forwards contacts to `hook`.
    ///
    /// ```ignore
    /// app.insert_resource(ContactAudioDispatch::with(MyHook::default()));
    /// ```
    pub fn with(hook: impl ContactAudioHook) -> Self {
        Self {
            hook: Some(Box::new(hook)),
        }
    }

    /// True when a hook is installed (the dispatch system will fan out).
    pub fn is_active(&self) -> bool {
        self.hook.is_some()
    }
}

/// Forward this frame's contacts to the installed hook. No-op (early
/// return) when no hook is installed, so the channel costs nothing
/// until a downstream app opts in.
pub fn dispatch_contact_audio(
    contacts: Res<AvatarContacts>,
    mut dispatch: ResMut<ContactAudioDispatch>,
) {
    let Some(hook) = dispatch.hook.as_mut() else {
        return;
    };
    hook.on_contacts(&contacts.samples);
}

/// Register the audio fanout channel. Inert until a downstream app
/// installs a [`ContactAudioHook`] via [`ContactAudioDispatch::with`].
pub fn build(app: &mut App) {
    app.init_resource::<ContactAudioDispatch>().add_systems(
        Update,
        dispatch_contact_audio
            .after(super::plugin::ContactProducerSet)
            .run_if(in_state(AppState::InGame)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::contact::{ContactPhase, SurfaceContact};
    use std::sync::{Arc, Mutex};

    fn sample(phase: ContactPhase) -> ContactSample {
        ContactSample {
            avatar: Entity::PLACEHOLDER,
            world_pos: Vec3::ZERO,
            world_vel: Vec3::ZERO,
            footprint_radius: 0.5,
            surface: SurfaceContact::Terrain {
                material_blend: [1.0, 0.0, 0.0, 0.0],
                normal: Vec3::Y,
            },
            intensity: 0.5,
            phase,
        }
    }

    #[derive(Default)]
    struct Spy {
        seen: Arc<Mutex<Vec<ContactPhase>>>,
    }
    impl ContactAudioHook for Spy {
        fn on_contacts(&mut self, samples: &[ContactSample]) {
            let mut g = self.seen.lock().unwrap();
            g.extend(samples.iter().map(|s| s.phase));
        }
    }

    #[test]
    fn default_dispatch_is_inactive() {
        let d = ContactAudioDispatch::default();
        assert!(!d.is_active());
    }

    #[test]
    fn with_installs_a_hook_that_receives_samples() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut d = ContactAudioDispatch::with(Spy { seen: seen.clone() });
        assert!(d.is_active());
        // Drive the hook directly (the system is a thin wrapper around
        // exactly this call).
        let samples = vec![sample(ContactPhase::Enter), sample(ContactPhase::Dwell)];
        d.hook.as_mut().unwrap().on_contacts(&samples);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![ContactPhase::Enter, ContactPhase::Dwell]
        );
    }
}
