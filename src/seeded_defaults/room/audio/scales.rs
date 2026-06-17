//! Just-intonation ratio tables for the theme-music voices. Extended
//! beyond pentatonic as authored themes need their own modes.

/// Major-pentatonic just ratios — warm rooms / bright themes chime major.
pub(super) const PENTATONIC_MAJOR: &[f32] = &[1.0, 1.125, 1.25, 1.5, 1.6667];
/// Minor-pentatonic just ratios — cool rooms / dark themes chime minor.
pub(super) const PENTATONIC_MINOR: &[f32] = &[1.0, 1.2, 1.3333, 1.5, 1.8];

/// Phrygian-ish ratios (minor with a flat 2nd) — tense, used by darker
/// / synth themes.
pub(super) const PHRYGIAN: &[f32] = &[1.0, 1.0667, 1.2, 1.3333, 1.5, 1.6, 1.7778];
/// Dorian-ish ratios (minor with a major 6th) — modal, folk / medieval.
pub(super) const DORIAN: &[f32] = &[1.0, 1.125, 1.2, 1.3333, 1.5, 1.6667, 1.7778];
