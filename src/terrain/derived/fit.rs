//! Fitting the theme's catalogue buildings to Berlin's places (#1587,
//! #1588): which entries a room grows, and at what size one fits a given
//! room.
//!
//! The pools are the lot layer's ([`crate::terrain::lots`]): the theme's
//! entries of a role, by the room's prosperity and escalation, a theme with
//! no landmark borrowing the settlements' fallback theme. A building is
//! drawn no bigger than the room it is given holds whichever way it turns:
//! at its catalogue size, or smaller in the lot layer's quarter-octave steps
//! down to [`SCALE_MIN`], and an entry too big even then gives way to the
//! next smaller one.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::RngCore;

use crate::catalogue::{CatalogueEntry, StructureRole, entries_for};
use crate::pds::RoomRecord;
use crate::seeded_defaults::{SceneCharacter, ThemeArchetype, fnv1a_64};
use crate::terrain::lots::{FALLBACK_THEME, fitted_scale, pool_for};

/// The smallest a derived building is drawn, as a share of its catalogue
/// size.
pub(crate) const SCALE_MIN: f32 = 0.5;

/// The room a plan is drawn for: the DID its catalogue items are built
/// for, and the seed its scene is drawn from - the theme, prosperity and
/// escalation its buildings, trees and props are picked by (#1589).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RoomScene {
    pub did: String,
    pub seed: u64,
}

impl RoomScene {
    /// The room `did` owns, as `record` was rolled: from the seed its base
    /// terrain was built from - a seeded record's own seed, a re-roll's as
    /// much as the DID's, which on Berlin's ground shapes nothing else - or,
    /// with no terrain, from the DID's. So a room re-rolled to a theme
    /// dresses Berlin in that theme, not its owner's.
    pub(crate) fn of(did: &str, record: Option<&RoomRecord>) -> Self {
        let seed = record
            .and_then(crate::pds::find_terrain_config)
            .map_or_else(|| fnv1a_64(did), |terrain| terrain.seed);
        RoomScene {
            did: did.to_owned(),
            seed,
        }
    }

    /// The room `did`'s own seed draws.
    #[cfg(test)]
    pub(crate) fn for_did(did: &str) -> Self {
        RoomScene {
            did: did.to_owned(),
            seed: fnv1a_64(did),
        }
    }

    /// The room's theme and its prosperity and escalation: the lot layer's,
    /// a theme with no landmark yet borrowing the settlements' fallback.
    pub(crate) fn theme(&self) -> (ThemeArchetype, (f32, f32)) {
        let scene = SceneCharacter::for_seed(self.seed);
        let theme = if entries_for(scene.theme, StructureRole::Landmark)
            .next()
            .is_some()
        {
            scene.theme
        } else {
            FALLBACK_THEME
        };
        (theme, (scene.prosperity, scene.escalation))
    }
}

/// The entries of `role` the room grows, smallest first (by [`radius`],
/// then slug), so a rank picks a size.
pub(crate) fn sized_pool(
    theme: ThemeArchetype,
    role: StructureRole,
    (prosperity, escalation): (f32, f32),
) -> Vec<&'static dyn CatalogueEntry> {
    let mut pool = pool_for(theme, role, prosperity, escalation);
    pool.sort_by(|a, b| {
        radius(*a)
            .total_cmp(&radius(*b))
            .then(a.slug().cmp(b.slug()))
    });
    pool
}

/// A landmark for a room of radius `room` (m): any of the pool's that fits.
pub(crate) fn pick_landmark(
    pool: &[&'static dyn CatalogueEntry],
    room: f32,
    rng: &mut ChaCha8Rng,
) -> Option<(&'static dyn CatalogueEntry, f32)> {
    let fitting: Vec<_> = pool
        .iter()
        .filter_map(|&entry| fitted(entry, room).map(|scale| (entry, scale)))
        .collect();
    (!fitting.is_empty()).then(|| fitting[rng.next_u32() as usize % fitting.len()])
}

/// An entry for a room of radius `room` from `pool`, smallest first: the one
/// at `rank` of the way up, a step either way, or the biggest smaller one
/// that fits.
pub(crate) fn pick_ranked(
    pool: &[&'static dyn CatalogueEntry],
    room: f32,
    rank: f32,
    rng: &mut ChaCha8Rng,
) -> Option<(&'static dyn CatalogueEntry, f32)> {
    let last = pool.len().checked_sub(1)?;
    let aim = ((rank * pool.len() as f32) as usize).min(last);
    let aim = match rng.next_u32() % 3 {
        0 => aim.saturating_sub(1),
        1 => aim,
        _ => (aim + 1).min(last),
    };
    pool[..=aim]
        .iter()
        .rev()
        .find_map(|&entry| fitted(entry, room).map(|scale| (entry, scale)))
}

/// The scale `entry` is drawn at in a room of radius `room` (m) - its fit
/// rounded down to a quarter-octave, at most its catalogue size - or `None`
/// where it outgrows the room even at [`SCALE_MIN`].
pub(crate) fn fitted(entry: &dyn CatalogueEntry, room: f32) -> Option<f32> {
    let reach = radius(entry);
    let scale = fitted_scale(room / reach, SCALE_MIN, 1.0);
    (scale * reach <= room).then_some(scale)
}

/// How far from its anchor an entry reaches at its catalogue size, turned
/// any way: its clearance, or the corner of its own half side where that is
/// nearer (an entry whose clearance is a spacing circle declares one).
pub(crate) fn radius(entry: &dyn CatalogueEntry) -> f32 {
    entry
        .footprint()
        .clearance
        .min(entry.lot_half_width() * std::f32::consts::SQRT_2)
        .max(0.5)
}

/// The lowest ground under a building reaching `reach` from `(x, z)`: its
/// anchor and the corners of the square inside its reach. A foundation then
/// bites into a slope rather than float over it.
pub(crate) fn footing(x: f32, z: f32, reach: f32, ground: &dyn Fn(f32, f32) -> f32) -> f32 {
    let corner = reach * std::f32::consts::FRAC_1_SQRT_2;
    [
        (0.0, 0.0),
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
    ]
    .into_iter()
    .map(|(sx, sz)| ground(x + sx * corner, z + sz * corner))
    .fold(f32::INFINITY, f32::min)
}

/// A DID whose own room grows `theme`'s buildings.
#[cfg(test)]
pub(crate) fn did_of(theme: ThemeArchetype) -> String {
    (0..10_000)
        .map(|i| format!("did:plc:derived{i}"))
        .find(|did| RoomScene::for_did(did).theme().0 == theme)
        .expect("a DID of the theme")
}

/// The cache key the tree of `slug` drawn at `scale_e4` files under, for a
/// plan whose keys start `prefix`. A record's generator key holds no `/`,
/// so no record generator files there.
pub(crate) fn cache_key(prefix: &str, slug: &str, scale_e4: i64) -> String {
    format!("{prefix}/{slug}@{scale_e4}")
}
