//! The baked clip library as an asset (#1057, decision recorded on #1055).
//!
//! Native embeds the engine's CC0 clip set through the sibling crate's
//! `builtin-clips` feature, so bodies have clips from the first frame. The
//! wasm build deliberately does not carry the 200 KiB artifact (#565: the
//! wasm heap never shrinks and every embedded byte is downloaded by every
//! visitor); it starts with an **empty** [`Clips`] — the rigged driver's
//! procedural-gait fallback covers the gap — and this module fetches
//! `assets/avatar.clips` through Bevy's own asset path (HTTP on wasm, disk
//! natively) and swaps the library in when it lands.
//!
//! Both targets run the same loader: on native the fetched library is
//! byte-identical to the builtin and the swap is a no-op, which keeps the
//! only divergence between the targets the *initial* value, not the code.
//!
//! The [`Clips`] resource is inserted here, by this crate. The sibling's
//! `AnimatorPlugin` is deliberately NOT added to the app — it drives a
//! single-subject viewer through a resource-level `Animator` and would also
//! insert its own `Clips` over anything already there (the documented
//! overwrite gotcha) plus an egui panel. Overlands' per-body driver is
//! [`super::rigged::drive_rigged_motion`].

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use bevy_symbios_avatar::Clips;
use symbios_avatar::ClipLibrary;

/// The clip artifact as a loadable asset: `assets/avatar.clips`, the
/// engine's `clips.bin` under an extension the loader can claim.
#[derive(Asset, TypePath)]
pub(super) struct ClipArchive(ClipLibrary);

/// Why `avatar.clips` failed to load. Hand-implemented rather than derived:
/// `thiserror` is not a direct dependency of this crate and two variants do
/// not earn one.
#[derive(Debug)]
pub(super) enum ClipArchiveError {
    Io(std::io::Error),
    Library(symbios_avatar::LibraryError),
}

impl std::fmt::Display for ClipArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "io: {error}"),
            Self::Library(error) => write!(f, "not a clip library: {error}"),
        }
    }
}

impl std::error::Error for ClipArchiveError {}

impl From<std::io::Error> for ClipArchiveError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<symbios_avatar::LibraryError> for ClipArchiveError {
    fn from(error: symbios_avatar::LibraryError) -> Self {
        Self::Library(error)
    }
}

/// Reads the engine's own serialized clip form.
#[derive(Default, TypePath)]
pub(super) struct ClipArchiveLoader;

impl AssetLoader for ClipArchiveLoader {
    type Asset = ClipArchive;
    type Settings = ();
    type Error = ClipArchiveError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(ClipArchive(ClipLibrary::read(&bytes)?))
    }

    fn extensions(&self) -> &[&str] {
        &["clips"]
    }
}

/// The in-flight fetch, removed once the library is swapped in.
#[derive(Resource)]
pub(super) struct PendingClips(Handle<ClipArchive>);

/// Kick the fetch at startup.
pub(super) fn request_clip_archive(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(PendingClips(assets.load("avatar.clips")));
}

/// Swap the fetched library into the [`Clips`] resource once it lands.
///
/// Failure leaves whatever is already there — the builtin set natively, the
/// empty library (and the procedural gait) on wasm — and stops asking, which
/// is the honest degradation: a missing asset is a deploy defect the log
/// should name once, not a panic and not a retry storm.
pub(super) fn install_clip_archive(
    mut commands: Commands,
    pending: Option<Res<PendingClips>>,
    assets: Res<AssetServer>,
    mut archives: ResMut<Assets<ClipArchive>>,
    mut clips: ResMut<Clips>,
) {
    let Some(pending) = pending else {
        return;
    };
    if let Some(archive) = archives.remove(&pending.0) {
        info!(
            "avatar clip library loaded: {} clips",
            archive.0.clips.len()
        );
        *clips = Clips(archive.0);
        commands.remove_resource::<PendingClips>();
    } else if assets.load_state(&pending.0).is_failed() {
        warn!("assets/avatar.clips failed to load — bodies keep the procedural gait");
        commands.remove_resource::<PendingClips>();
    }
}
